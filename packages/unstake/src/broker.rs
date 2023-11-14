use std::{
    cmp::max,
    ops::{Div, Mul, Sub},
    time::Duration,
};

use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, Deps, DepsMut, QuerierWrapper, StdResult, Storage, Uint128};
use cw_storage_plus::Item;

use crate::ContractError;

static BROKER: Item<Broker> = Item::new("broker");

// The amount of the staked asset that we have as a reserve to pay excess interest
static RESERVES: Item<Uint128> = Item::new("reserves");

static YEAR_SECONDS: u128 = 365 * 24 * 60 * 60;

/// The Broker is responsible for managing protocol reserves, and making Unstaking offers
#[cw_serde]
pub struct Broker {
    pub vault: Addr,
    /// The minimum rate that the Broker will offer. Typically this should be set to the utilization target
    /// of the GHOST vault, or maybe slightly above. Any Unstakes that have a net interest of less than this
    /// will contribute to protocol reserves
    pub min_rate: Decimal,

    /// The length of time that an unbonding request must wait
    pub duration: Duration,
}

impl Broker {
    pub fn load(storage: &dyn Storage) -> StdResult<Self> {
        BROKER.load(storage)
    }

    /// Make an offer for a givan `amount` of the staked token
    pub fn offer(&self, deps: Deps, amount: Uint128) -> Result<Offer, ContractError> {
        let redemption_rate = self.fetch_redemption_rate(deps.querier)?;
        let current_rate = self.fetch_current_interest_rate(deps.querier)?;
        let max_rate = self.fetch_max_interest_rate(deps.querier)?;

        // Calculate the value of the Unstaked amount, in terms of the underlying asset. I.e. the max amount we'll need to borrow
        let value = amount.mul(redemption_rate);

        // For now we'll naively assume that the borrow rate will stay fixed for the duration of the unbond.
        // During periods of high interest, Unstakes will cost more and a user will have to wait for the rate
        // to fall if they want a more favourable rate.
        let offer_rate = max(current_rate, self.min_rate);
        let max_rate_shortfall = max_rate
            .checked_sub(current_rate)
            .map_err(ContractError::RateOverflow)?;

        // Ensure we have enough reserves available to cover the max potential shortfall - ie the lend APR spiking to max
        // in the following block, and remaining there for the whole period
        // This is something that we can look to relax in due course, but for now it provides an absolute guarantee of solvency
        let reserve_allocation = self.interest_amount(value, max_rate_shortfall);
        let available_reserve = RESERVES.load(deps.storage).unwrap_or_default();

        if reserve_allocation.gt(&available_reserve) {
            return Err(ContractError::InsufficentReserves {});
        };

        // Calculate the total that we'll charge in up-front interest
        let fee = self.interest_amount(value, offer_rate);

        // The actual offer amount, and therefore the amount that we borrow from GHOST, is less then the `value` that we
        // calculated the total interest amount on. The larger the current_rate, the larger the fee, the less we're actually
        // borrowing, so the actual amount of interest paid will be lower.
        // Therefore when the unbonded tokens return, we will have a surplus after the debt has been repaid.
        let offer = Offer {
            amount: value.sub(fee),
            reserve_allocation,
            fee,
        };

        Ok(offer)
    }

    /// Commits the offer, deducts the reserve allocation from the total reservce, and returns
    /// messages that will instantiate the delegate contract with the debt tokens and ask tokens
    pub fn accept_offer(&self, deps: DepsMut, offer: &Offer) -> StdResult<()> {
        let mut available_reserve = RESERVES.load(deps.storage).unwrap_or_default();
        available_reserve -= offer.reserve_allocation;
        RESERVES.save(deps.storage, &available_reserve)?;
        Ok(())
    }

    /// Receives the original offer, debt tokens, and returned unbonded tokens from the delegate,
    /// reconciles the reserves
    pub fn close_offer(
        &self,
        deps: DepsMut,
        offer: &Offer,
        debt_tokens: Uint128,
        returned_tokens: Uint128,
        protocol_fee: Decimal,
    ) -> StdResult<(Uint128, Uint128)> {
        let debt_rate = self.fetch_debt_rate(deps.querier)?;
        let debt_amount = debt_tokens * debt_rate;
        let mut available_reserve = RESERVES.load(deps.storage).unwrap_or_default();

        // Start off by naively re-allocating the reserve back to the total
        available_reserve += offer.reserve_allocation;
        let protocol_fee_amount: Uint128;

        if debt_amount.gt(&returned_tokens) {
            // Interest rate has been higher than quoted. Handle the loss.
            protocol_fee_amount = Uint128::zero();
            available_reserve -= debt_amount.sub(returned_tokens)
        } else {
            // We've made profit on this Unstake. Distribute accordingly
            // NB we only take profit if there is a surplus after the unbonding, otherwise it would deplete
            // reserves unncessarily
            let profit = returned_tokens.sub(debt_amount);
            protocol_fee_amount = protocol_fee.mul(profit);
            let reserve_contribution = profit.sub(protocol_fee_amount);
            available_reserve += reserve_contribution
        }
        Ok((debt_amount, protocol_fee_amount))
    }

    fn interest_amount(&self, amount: Uint128, rate: Decimal) -> Uint128 {
        amount
            .mul(rate)
            .mul(Uint128::from(self.duration.as_secs()))
            .div(Uint128::from(YEAR_SECONDS))
    }

    fn fetch_debt_rate(&self, query: QuerierWrapper) -> StdResult<Decimal> {
        let status: kujira_ghost::receipt_vault::StatusResponse = query.query_wasm_smart(
            self.vault.to_string(),
            &kujira_ghost::receipt_vault::QueryMsg::Status {},
        )?;

        Ok(status.debt_share_ratio)
    }

    fn fetch_current_interest_rate(&self, query: QuerierWrapper) -> StdResult<Decimal> {
        let status: kujira_ghost::receipt_vault::StatusResponse = query.query_wasm_smart(
            self.vault.to_string(),
            &kujira_ghost::receipt_vault::QueryMsg::Status {},
        )?;

        Ok(status.rate)
    }
    fn fetch_max_interest_rate(&self, _query: QuerierWrapper) -> StdResult<Decimal> {
        // TODO: Publish & ise new interest rate params
        // let rates: kujira_ghost::receipt_vault::InterestParamsResponse = query.query_wasm_smart(
        //     self.vault.to_string(),
        //     &kujira_ghost::receipt_vault::QueryMsg::InterestParams {},
        // )?;
        Ok(Decimal::from_ratio(3u128, 1u128))
    }
    fn fetch_redemption_rate(&self, _query: QuerierWrapper) -> StdResult<Decimal> {
        todo!()
    }
}

/// The details of an offer returned by the Broker
#[cw_serde]
pub struct Offer {
    /// The amount that we can safely borrow from GHOST and return to the Unstaker
    pub amount: Uint128,

    /// The amount of the offer amount that has been retained as a fee to cover interest.
    /// amount + fee_amount == unbond_amount * redemption_rate
    pub fee: Uint128,

    /// The amount of reserves allocated to this offer
    pub reserve_allocation: Uint128,
}
