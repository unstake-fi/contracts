use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Decimal, StdResult, Storage};
use cw_storage_plus::Item;
use monetary::{AmountU128, Rate};
use unstake::denoms::{Base, Rcpt, Rsv};

#[cw_serde]
pub struct State {
    pub deployed: AmountU128<Base>,
    pub available: AmountU128<Rcpt>,
    pub reserve_redemption_ratio: Rate<Base, Rsv>,
}

impl State {
    pub fn new() -> Self {
        State {
            deployed: AmountU128::zero(),
            available: AmountU128::zero(),
            reserve_redemption_ratio: Rate::new(Decimal::one()).unwrap(),
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
