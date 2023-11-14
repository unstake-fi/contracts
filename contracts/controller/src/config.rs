use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Deps, DepsMut, StdResult};
use cw_storage_plus::Item;
use kujira::Denom;
use unstake::controller::InstantiateMsg;

static CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub delegate_code_id: u64,
    pub vault_address: Addr,
    pub offer_denom: Denom,
    pub ask_denom: Denom,
}

impl Config {
    pub fn load(deps: Deps) -> StdResult<Self> {
        CONFIG.load(deps.storage)
    }

    pub fn save(&self, deps: DepsMut) -> StdResult<()> {
        CONFIG.save(deps.storage, self)
    }

    pub fn debt_denom(&self) -> Denom {
        Denom::from(format!("factory/{}/udebt", self.vault_address))
    }
}

impl From<InstantiateMsg> for Config {
    fn from(value: InstantiateMsg) -> Self {
        Self {
            owner: value.owner,
            delegate_code_id: value.delegate_code_id,
            vault_address: value.vault_address,
            offer_denom: value.offer_denom,
            ask_denom: value.ask_denom,
        }
    }
}
