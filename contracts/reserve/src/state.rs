use cosmwasm_schema::cw_serde;
use cosmwasm_std::{StdResult, Storage};
use cw_storage_plus::{Item, Map};
use monetary::{AmountU128, Rate};
use unstake::denoms::{Base, LegacyRsv, Rcpt, Rsv};

#[cw_serde]
pub struct State {
    pub deployed: AmountU128<Base>,
    pub available: AmountU128<Rcpt>,
}

impl State {
    pub fn new() -> Self {
        State {
            deployed: AmountU128::zero(),
            available: AmountU128::zero(),
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

pub const LEGACY_DENOMS: Map<String, Rate<Rsv, LegacyRsv>> = Map::new("legacy_denoms");
