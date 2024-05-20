use cosmwasm_schema::cw_serde;
use cosmwasm_std::{coin, Coin, CustomQuery, Decimal, DepsMut, StdResult, Storage};
use cw_storage_plus::Item;
use cw_utils::NativeBalance;
use monetary::{AmountU128, CheckedCoin, Exchange};
use std::{
    cmp::{max, min},
    ops::{Add, Sub},
};

use crate::{
    controller::InstantiateMsg,
    denoms::{Ask, Base, Debt, Rcpt},
    rates::Rates,
    reserve::StatusResponse as ReserveStatus,
    ContractError,
};

const BROKER: Item<Broker> = Item::new("broker");

// The total amount of (base, quote) tokens that have been (initiated, returned) from unbonding
const TOTALS: Item<(AmountU128<Ask>, AmountU128<Base>)> = Item::new("totals");

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
    pub fn offer<T: CustomQuery>(
        &self,
        reserve_status: &ReserveStatus,
        rates: &Rates,
        unbond_amount: AmountU128<Ask>,
    ) -> Result<Offer, ContractError> {
        let current_rate = rates.vault_interest;
        let max_rate = rates.vault_max_interest;

        // Calculate the value of the Unstaked amount, in terms of the underlying asset. I.e. the max amount we'll need to borrow
        let value = unbond_amount.mul_floor(&rates.provider_redemption);

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
        let reserve_requirement = self.interest_amount(value, max_rate_shortfall);
        let ghost_reserve_available = AmountU128::<Rcpt>::new(reserve_status.reserves_available);
        let reserve_available = ghost_reserve_available.mul_floor(&rates.vault_deposit);

        // Calculate the total that we'll charge in up-front interest
        let fee = self.interest_amount(value, offer_rate);

        // We have enough reserve to offer at the clamped offer_rate
        if reserve_available.gt(&reserve_requirement) {
            // The actual offer amount, and therefore the amount that we borrow from GHOST, is less then the `value` that we
            // calculated the total interest amount on. The larger the current_rate, the larger the fee, the less we're actually
            // borrowing, so the actual amount of interest paid will be lower.
            // Therefore when the unbonded tokens return, we will have a surplus after the debt has been repaid.
            let offer = Offer {
                unbond_amount,
                offer_amount: value.sub(fee),
                reserve_allocation: reserve_requirement,
                fee,
            };

            return Ok(offer);
        }

        // We can't offer at the current rate, calculate the best rate we can offer given the reserves available
        // Consume the whole reserve, and make a best offer
        let reserve_allocation = reserve_available;
        // Allocate the shortfall to fee, deduct from the amount returned
        let reserve_shortfall = reserve_requirement.sub(reserve_available);
        let offer = Offer {
            unbond_amount,
            offer_amount: value.sub(fee).sub(reserve_shortfall),
            reserve_allocation,
            fee: fee.add(reserve_shortfall),
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
        debt_coin: CheckedCoin<Debt>,
        base_coin: CheckedCoin<Base>,
    ) -> Result<(Vec<Coin>, AmountU128<Base>, AmountU128<Base>), ContractError> {
        let CheckedCoin {
            denom: debt_denom,
            amount: debt_tokens,
        } = debt_coin;
        let CheckedCoin {
            denom: base_denom,
            amount: mut returned_tokens,
        } = base_coin;

        let (total_base, mut total_quote) = TOTALS.load(deps.storage).unwrap_or_default();
        total_quote += returned_tokens
            .checked_sub(offer.reserve_allocation)
            .unwrap_or_default();
        TOTALS.save(deps.storage, &(total_base, total_quote))?;

        let debt_rate = rates.vault_debt;
        let debt_amount = debt_tokens.mul_ceil(&debt_rate);

        // We *should* always have enough to repay the GHOST debt -
        // the reserve prepaid part of the debt, and the fee should cover the rest.
        if debt_amount.gt(&returned_tokens) {
            return Err(ContractError::Insolvent {
                debt_remaining: debt_amount.sub(returned_tokens).uint128(),
            });
        }

        // Ok, now let's proceed to allocate the returned tokens in priority.

        // Number one. Repay GHOST
        returned_tokens -= debt_amount;
        let mut repay_funds = NativeBalance(vec![
            coin(debt_amount.u128(), base_denom.to_string()),
            coin(debt_tokens.u128(), debt_denom.to_string()),
        ]);
        repay_funds.normalize();

        // Number two. Repay the solvency fund as much as possible
        let reserve_allocation = min(offer.reserve_allocation, returned_tokens);
        // Finally what's left is our revenue.
        let fee_amount = returned_tokens.sub(reserve_allocation);

        Ok((repay_funds.into_vec(), reserve_allocation, fee_amount))
    }

    fn interest_amount(&self, amount: AmountU128<Base>, rate: Decimal) -> AmountU128<Base> {
        let rate = rate * Decimal::from_ratio(self.duration, YEAR_SECONDS);
        AmountU128::new(amount.uint128().mul_ceil(rate))
    }
}

/// The details of an offer returned by the Broker
#[cw_serde]
pub struct Offer {
    /// The amount requested for unbonding
    pub unbond_amount: AmountU128<Ask>,

    /// The amount that we can safely borrow from GHOST and return to the Unstaker
    pub offer_amount: AmountU128<Base>,

    /// The amount of the offer amount that has been retained as a fee to cover interest.
    /// amount + fee_amount == unbond_amount * redemption_rate
    pub fee: AmountU128<Base>,

    /// The amount of reserves allocated to this offer
    pub reserve_allocation: AmountU128<Base>,
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
    pub total_base: AmountU128<Ask>,
    /// The total amount of quote asset that has been returned from unbonding
    pub total_quote: AmountU128<Base>,
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
