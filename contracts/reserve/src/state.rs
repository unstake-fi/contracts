use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal, StdResult, Storage, Uint128};
use cw_storage_plus::Item;

#[cw_serde]
pub struct State {
    /// Total deposits, denominated in reserve denom.
    pub total_deposits: Uint128,
    /// Ratio of reserve token to redeemable ghost receipt tokens.
    pub reserve_redemption_ratio: Decimal,
}

impl State {
    pub fn new() -> Self {
        State {
            total_deposits: Uint128::zero(),
            reserve_redemption_ratio: Decimal::one(),
        }
    }

    pub fn load(storage: &dyn Storage) -> StdResult<Self> {
        STATE.load(storage)
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        STATE.save(storage, self)
    }
}

impl Default for State {
    fn default() -> Self {
        Self::new()
    }
}

pub const STATE: Item<State> = Item::new("state");
