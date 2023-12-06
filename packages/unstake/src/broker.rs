use cosmwasm_schema::cw_serde;
use cosmwasm_std::{CustomQuery, Decimal, Deps, DepsMut, StdResult, Storage, Uint128};
use cw_storage_plus::Item;
use std::{
    cmp::{max, min},
    ops::{Add, Sub},
};

use crate::{controller::InstantiateMsg, rates::Rates, ContractError};

const BROKER: Item<Broker> = Item::new("broker");

/// The amount of the staked asset that we have as a reserve to pay excess interest
/// (available, deployed)
const RESERVES: Item<(Uint128, Uint128)> = Item::new("reserves");

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

    pub fn fund_reserves(storage: &mut dyn Storage, amount: Uint128) -> StdResult<()> {
        let (mut reserves, deployed) = RESERVES.load(storage).unwrap_or_default();
        reserves += amount;
        RESERVES.save(storage, &(reserves, deployed))
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
        deps: Deps<T>,
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
        let reserve_requirement = self.interest_amount(value, max_rate_shortfall);
        let (available_reserve, _) = RESERVES.load(deps.storage).unwrap_or_default();

        // Calculate the total that we'll charge in up-front interest
        let fee = self.interest_amount(value, offer_rate);

        // We have enough reserve to offer at the clamped offer_rate
        if available_reserve.gt(&reserve_requirement) {
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
        let reserve_allocation = available_reserve;
        // Allocate the shortfall to fee, deduct from the amount returned
        let reserve_shortfall = reserve_requirement.sub(available_reserve);
        let offer = Offer {
            unbond_amount,
            offer_amount: value.sub(fee).sub(reserve_shortfall),
            reserve_allocation,
            fee: fee.add(reserve_shortfall),
        };

        Ok(offer)
    }

    /// Commits the offer, deducts the reserve allocation from the total reservce, and returns
    /// messages that will instantiate the delegate contract with the debt tokens and ask tokens
    pub fn accept_offer<T: CustomQuery>(
        &self,
        deps: DepsMut<T>,
        offer: &Offer,
    ) -> Result<(), ContractError> {
        let (mut available_reserve, mut deployed) = RESERVES.load(deps.storage).unwrap_or_default();
        available_reserve = available_reserve
            .checked_sub(offer.reserve_allocation)
            .map_err(|_| ContractError::InsufficentReserves {})?;
        deployed = deployed.checked_add(offer.reserve_allocation)?;
        RESERVES.save(deps.storage, &(available_reserve, deployed))?;

        let (mut total_base, total_quote) = TOTALS.load(deps.storage).unwrap_or_default();
        total_base += offer.unbond_amount;
        TOTALS.save(deps.storage, &(total_base, total_quote))?;

        Ok(())
    }

    /// Receives the original offer, debt tokens, and returned unbonded tokens from the delegate,
    /// reconciles the reserves
    pub fn close_offer<T: CustomQuery>(
        &self,
        deps: DepsMut<T>,
        rates: &Rates,
        offer: &Offer,
        debt_tokens: Uint128,
        mut returned_tokens: Uint128,
        protocol_fee: Decimal,
    ) -> Result<(Uint128, Uint128), ContractError> {
        let (total_base, mut total_quote) = TOTALS.load(deps.storage).unwrap_or_default();
        total_quote += returned_tokens
            .checked_sub(offer.reserve_allocation)
            .unwrap_or_default();
        TOTALS.save(deps.storage, &(total_base, total_quote))?;

        let debt_rate = rates.vault_debt;
        let debt_amount = debt_tokens.mul_ceil(debt_rate);
        let (mut available_reserve, mut deployed_reserve) =
            RESERVES.load(deps.storage).unwrap_or_default();

        // We will always have enough to repay the GHOST debt -
        // the fee + reserve allocation will cover the potential shortfall

        // But check anyway

        if debt_amount.gt(&returned_tokens) {
            return Err(ContractError::Insolvent {
                debt_remaining: debt_amount.sub(returned_tokens),
            });
        }

        // Ok, now let's proceed to allocate the returned tokens in priority.

        // Number one. Repay GHOST
        returned_tokens -= debt_amount;

        // Number two. Repay the solvency fund as much as possible
        let reserve_allocation = min(offer.reserve_allocation, returned_tokens);
        available_reserve += reserve_allocation;
        deployed_reserve -= reserve_allocation;
        returned_tokens -= reserve_allocation;

        // Finally see what we can take as a fee. Consume whatever's left, in the case this is a big profit

        // Calculate the protocol revenue
        let fee_amount = returned_tokens.mul_floor(protocol_fee);

        // And what's left to be allocated as a reserve
        let fee_reserve = returned_tokens.sub(fee_amount);

        available_reserve += fee_reserve;

        RESERVES.save(deps.storage, &(available_reserve, deployed_reserve))?;

        Ok((debt_amount, fee_amount))
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

impl Into<String> for Offer {
    fn into(self) -> String {
        format!(
            "unbond_amount:{},offer_amount:{},fee:{},reserve_allocation:{}",
            self.unbond_amount, self.offer_amount, self.fee, self.reserve_allocation
        )
    }
}

#[cw_serde]
pub struct Status {
    /// The total amount of base asset that has been requested for unbonding
    pub total_base: Uint128,
    /// The total amount of quote asset that has been returned from unbonding
    pub total_quote: Uint128,
    /// The amount of reserve currently available for new Unstakes
    pub reserve_available: Uint128,
    /// The amount of reserve currently deployed in in-flight Unstakes
    pub reserve_deployed: Uint128,
}

impl Status {
    pub fn load(storage: &dyn Storage) -> Self {
        let (total_base, total_quote) = TOTALS.load(storage).unwrap_or_default();
        let (reserve_available, reserve_deployed) = RESERVES.load(storage).unwrap_or_default();
        Self {
            total_base,
            total_quote,
            reserve_available,
            reserve_deployed,
        }
    }
}
