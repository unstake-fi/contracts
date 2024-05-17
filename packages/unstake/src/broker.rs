use cosmwasm_schema::cw_serde;
use cosmwasm_std::{coin, Coin, CustomQuery, Decimal, DepsMut, StdResult, Storage, Uint128};
use cw_storage_plus::Item;
use cw_utils::NativeBalance;
use kujira_ghost::math::{calculate_removed_debt, debt_to_liability};
use std::{
    cmp::{max, min},
    ops::{Add, Sub},
};

use crate::{
    controller::InstantiateMsg, rates::Rates, reserve::StatusResponse as ReserveStatus,
    ContractError,
};

const BROKER: Item<Broker> = Item::new("broker");

// The total amount of (base, quote) tokens that have been (initiated, returned) from unbonding
const TOTALS: Item<(Uint128, Uint128)> = Item::new("totals");

const YEAR_SECONDS: u128 = 365 * 24 * 60 * 60;

/// The Broker is responsible for managing protocol reserves, and making Unstaking offers
#[cw_serde]
pub struct Broker {
    /// The minimum rate that the Broker will offer. Typically this should be set to the utilization target
    /// of the GHOST vault, or maybe slightly above. Any Unstakes that have a net interest of less than this
    /// will contribute to protocol reserves
    pub min_rate: Decimal,

    /// The length of time in seconds that an unbonding request must wait
    pub duration: u64,
}

impl From<InstantiateMsg> for Broker {
    fn from(value: InstantiateMsg) -> Self {
        Self {
            min_rate: value.min_rate,
            duration: value.unbonding_duration,
        }
    }
}

impl Broker {
    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        BROKER.save(storage, self)
    }

    pub fn load(storage: &dyn Storage) -> StdResult<Self> {
        BROKER.load(storage)
    }

    pub fn update(&mut self, min_rate: Option<Decimal>, duration: Option<u64>) {
        if let Some(min_rate) = min_rate {
            self.min_rate = min_rate
        }

        if let Some(duration) = duration {
            self.duration = duration
        }
    }

    /// Make an offer for a givan `amount` of the staked token
    pub fn offer(
        &self,
        reserve_status: &ReserveStatus,
        rates: &Rates,
        unbond_amount: Uint128,
    ) -> Result<Offer, ContractError> {
        let current_rate = rates.vault_interest;
        let max_rate = rates.vault_max_interest;

        // Calculate the value of the Unstaked amount, in terms of the underlying asset. I.e. the max amount we'll need to borrow
        let value = unbond_amount.mul_floor(rates.provider_redemption);

        // For now we'll naively assume that the borrow rate will stay fixed for the duration of the unbond.
        // During periods of high interest, Unstakes will cost more and a user will have to wait for the rate
        // to fall if they want a more favourable rate.
        let offer_rate = max(current_rate, self.min_rate);
        let max_rate_shortfall = max_rate
            .checked_sub(offer_rate)
            .map_err(ContractError::RateOverflow)?;

        // Ensure we have enough reserves available to cover the max potential shortfall - ie the lend APR spiking to max
        // in the following block, and remaining there for the whole period
        // This is something that we can look to relax in due course, but for now it provides an absolute guarantee of solvency
        let base_required = self.interest_amount(value, max_rate_shortfall);
        // Round up, to ensure full coverage.
        let ghost_reserve_requirement = base_required.mul_ceil(rates.vault_deposit);
        let ghost_reserve_available = reserve_status.reserves_available;

        // Calculate the total that we'll charge in up-front interest
        let fee = self.interest_amount(value, offer_rate);

        // We have enough reserve to offer at the clamped offer_rate
        if ghost_reserve_available > ghost_reserve_requirement {
            // The actual offer amount, and therefore the amount that we borrow from GHOST, is less then the `value` that we
            // calculated the total interest amount on. The larger the current_rate, the larger the fee, the less we're actually
            // borrowing, so the actual amount of interest paid will be lower.
            // Therefore when the unbonded tokens return, we will have a surplus after the debt has been repaid.
            let offer = Offer {
                unbond_amount,
                offer_amount: value.sub(fee),
                reserve_allocation: ghost_reserve_requirement,
                fee,
            };

            return Ok(offer);
        }

        // We can't offer at the current rate, calculate the best rate we can offer given the reserves available
        // Consume the whole reserve, and make a best offer
        let ghost_reserve_allocation = ghost_reserve_available;
        // Allocate the shortfall to fee, deduct from the amount returned
        let ghost_reserve_shortfall = ghost_reserve_requirement.sub(ghost_reserve_available);
        // Round up, maximizing fee to cover worst case.
        let shortfall_fee = ghost_reserve_shortfall.div_ceil(rates.vault_deposit);
        let offer = Offer {
            unbond_amount,
            offer_amount: value.sub(fee).sub(shortfall_fee),
            reserve_allocation: ghost_reserve_allocation,
            fee: fee.add(shortfall_fee),
        };

        Ok(offer)
    }

    /// Tallies an accepted offer and updates the running totals.
    pub fn accept_offer<T: CustomQuery>(
        &self,
        deps: DepsMut<T>,
        offer: &Offer,
    ) -> Result<(), ContractError> {
        TOTALS.update(deps.storage, |(mut total_base, total_quote)| {
            total_base += offer.unbond_amount;
            StdResult::Ok((total_base, total_quote))
        })?;

        Ok(())
    }

    /// Receives the original offer, debt tokens, and returned unbonded tokens from the delegate,
    /// reconciles the reserves
    pub fn close_offer<T: CustomQuery>(
        &self,
        deps: DepsMut<T>,
        rates: &Rates,
        offer: &Offer,
        debt_coin: Coin,
        base_coin: Coin,
        ghost_coin: Coin,
    ) -> Result<(Vec<Coin>, Uint128, Uint128), ContractError> {
        let Coin {
            denom: debt_denom,
            amount: debt_tokens,
        } = debt_coin;
        let Coin {
            denom: base_denom,
            amount: returned_base_tokens,
        } = base_coin;
        let Coin {
            denom: ghost_denom,
            amount: returned_ghost_tokens,
        } = ghost_coin;

        TOTALS.update(deps.storage, |(total_base, mut total_quote)| {
            total_quote += returned_ghost_tokens
                .sub(offer.reserve_allocation)
                .mul_floor(rates.vault_deposit);
            StdResult::Ok((total_base, total_quote))
        })?;

        let debt_rate = rates.vault_debt;
        let rcpt_to_debt_rate = debt_rate / rates.vault_deposit;

        let debt_value = debt_tokens.mul_ceil(debt_rate);
        let returned_value =
            returned_base_tokens.add(returned_ghost_tokens.mul_floor(rates.vault_deposit));

        // We *should* always have enough to repay the GHOST debt -
        // the fee + reserve allocation will cover the potential shortfall
        if debt_value.gt(&returned_value) {
            return Err(ContractError::Insolvent {
                debt_remaining: debt_value.sub(returned_value),
            });
        }

        // Ok, now let's proceed to allocate the returned tokens in priority.

        // Number one, begin to repay GHOST using returned unbonded tokens
        let remaining_debt_tokens = calculate_removed_debt(returned_base_tokens, rates.vault_debt);
        // Number two, repay the rest of GHOST using the reserve & fee allocation
        let max_removed_debt = calculate_removed_debt(returned_ghost_tokens, rcpt_to_debt_rate);
        let removed_debt = max_removed_debt.min(remaining_debt_tokens);

        let remaining_ghost_tokens =
            debt_to_liability(max_removed_debt.sub(removed_debt), rcpt_to_debt_rate);
        let ghost_repay_tokens = returned_ghost_tokens.sub(remaining_ghost_tokens);

        let mut repay_funds = NativeBalance(vec![
            coin(returned_base_tokens.u128(), base_denom),
            coin(ghost_repay_tokens.u128(), ghost_denom),
            coin(debt_tokens.u128(), debt_denom),
        ]);
        repay_funds.normalize();

        // Number three, repay the reserve as much as possible
        let reserve_allocation = min(offer.reserve_allocation, remaining_ghost_tokens);
        // The remaining ghost tokens after repaying the reserve is revenue.
        let ghost_fee_amount = remaining_ghost_tokens.sub(reserve_allocation);

        Ok((repay_funds.into_vec(), reserve_allocation, ghost_fee_amount))
    }

    /// Receives the original offer, debt tokens, and returned unbonded tokens from the delegate,
    /// reconciles the reserves
    #[deprecated]
    pub fn close_legacy_offer<T: CustomQuery>(
        &self,
        deps: DepsMut<T>,
        rates: &Rates,
        offer: &Offer,
        debt_coin: Coin,
        base_coin: Coin,
    ) -> Result<(Vec<Coin>, Uint128, Uint128), ContractError> {
        let Coin {
            denom: debt_denom,
            amount: debt_tokens,
        } = debt_coin;
        let Coin {
            denom: base_denom,
            amount: mut returned_tokens,
        } = base_coin;

        let (total_base, mut total_quote) = TOTALS.load(deps.storage).unwrap_or_default();
        total_quote += returned_tokens
            .checked_sub(offer.reserve_allocation)
            .unwrap_or_default();
        TOTALS.save(deps.storage, &(total_base, total_quote))?;

        let debt_rate = rates.vault_debt;
        let debt_amount = debt_tokens.mul_ceil(debt_rate);

        // We *should* always have enough to repay the GHOST debt -
        // the fee + reserve allocation will cover the potential shortfall
        if debt_amount.gt(&returned_tokens) {
            return Err(ContractError::Insolvent {
                debt_remaining: debt_amount.sub(returned_tokens),
            });
        }

        // Ok, now let's proceed to allocate the returned tokens in priority.

        // Number one. Repay GHOST
        returned_tokens -= debt_amount;
        let mut repay_funds = NativeBalance(vec![
            coin(debt_amount.u128(), base_denom),
            coin(debt_tokens.u128(), debt_denom),
        ]);
        repay_funds.normalize();

        // Number two. Repay the solvency fund as much as possible
        let reserve_allocation = min(offer.reserve_allocation, returned_tokens);
        // Finally what's left is our revenue.
        let fee_amount = returned_tokens.sub(reserve_allocation);

        Ok((repay_funds.into_vec(), reserve_allocation, fee_amount))
    }

    fn interest_amount(&self, amount: Uint128, rate: Decimal) -> Uint128 {
        amount.mul_ceil(rate).mul_ceil(Decimal::from_ratio(
            Uint128::from(self.duration),
            Uint128::from(YEAR_SECONDS),
        ))
    }
}

/// The details of an offer returned by the Broker
#[cw_serde]
pub struct Offer {
    /// The amount requested for unbonding
    pub unbond_amount: Uint128,

    /// The amount that we can safely borrow from GHOST and return to the Unstaker
    pub offer_amount: Uint128,

    /// The amount of the offer amount that has been retained as a fee to cover interest.
    /// amount + fee_amount == unbond_amount * redemption_rate
    pub fee: Uint128,

    /// The amount of reserves allocated to this offer
    pub reserve_allocation: Uint128,
}

impl From<Offer> for String {
    fn from(value: Offer) -> Self {
        format!(
            "unbond_amount:{},offer_amount:{},fee:{},reserve_allocation:{}",
            value.unbond_amount, value.offer_amount, value.fee, value.reserve_allocation
        )
    }
}

#[cw_serde]
pub struct Status {
    /// The total amount of base asset that has been requested for unbonding
    pub total_base: Uint128,
    /// The total amount of quote asset that has been returned from unbonding
    pub total_quote: Uint128,
}

impl Status {
    pub fn load(storage: &dyn Storage) -> Self {
        let (total_base, total_quote) = TOTALS.load(storage).unwrap_or_default();
        Self {
            total_base,
            total_quote,
        }
    }
}
