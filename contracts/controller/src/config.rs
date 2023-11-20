use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, CustomQuery, Decimal, Deps, StdResult, Storage};
use cw_storage_plus::Item;
use kujira::Denom;
use unstake::{adapter::Adapter, controller::InstantiateMsg};

static CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub protocol_fee: Decimal,
    pub delegate_code_id: u64,
    pub vault_address: Addr,
    pub offer_denom: Denom,
    pub ask_denom: Denom,
    pub adapter: Adapter,
}

impl Config {
    pub fn load<T: CustomQuery>(deps: Deps<T>) -> StdResult<Self> {
        CONFIG.load(deps.storage)
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        CONFIG.save(storage, self)
    }

    pub fn debt_denom(&self) -> Denom {
        Denom::from(format!("factory/{}/udebt", self.vault_address))
    }
}

impl From<InstantiateMsg> for Config {
    fn from(value: InstantiateMsg) -> Self {
        Self {
            owner: value.owner,
            protocol_fee: value.protocol_fee,
            delegate_code_id: value.delegate_code_id,
            vault_address: value.vault_address,
            offer_denom: value.offer_denom,
            ask_denom: value.ask_denom,
            adapter: value.adapter,
        }
    }
}
