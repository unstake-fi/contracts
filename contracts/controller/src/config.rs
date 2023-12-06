use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, StdResult, Storage};
use cw_storage_plus::Item;
use kujira::Denom;
use unstake::{
    adapter::Adapter,
    controller::{ConfigResponse, InstantiateMsg},
};

static CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub protocol_fee: Decimal,
    pub protocol_fee_address: Addr,
    pub delegate_code_id: u64,
    pub vault_address: Addr,
    pub offer_denom: Denom,
    pub ask_denom: Denom,
    pub adapter: Adapter,
}

impl Config {
    pub fn load(storage: &dyn Storage) -> StdResult<Self> {
        CONFIG.load(storage)
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        CONFIG.save(storage, self)
    }

    pub fn update(
        &mut self,
        owner: Option<Addr>,
        protocol_fee: Option<Decimal>,
        protocol_fee_address: Option<Addr>,
        delegate_code_id: Option<u64>,
    ) {
        if let Some(owner) = owner {
            self.owner = owner
        }
        if let Some(protocol_fee) = protocol_fee {
            self.protocol_fee = protocol_fee
        }

        if let Some(protocol_fee_address) = protocol_fee_address {
            self.protocol_fee_address = protocol_fee_address
        }

        if let Some(delegate_code_id) = delegate_code_id {
            self.delegate_code_id = delegate_code_id
        }
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
            protocol_fee_address: value.protocol_fee_address,
            delegate_code_id: value.delegate_code_id,
            vault_address: value.vault_address,
            offer_denom: value.offer_denom,
            ask_denom: value.ask_denom,
            adapter: value.adapter,
        }
    }
}

impl From<Config> for ConfigResponse {
    fn from(value: Config) -> Self {
        Self {
            owner: value.owner,
            protocol_fee: value.protocol_fee,
            protocol_fee_address: value.protocol_fee_address,
            delegate_code_id: value.delegate_code_id,
            vault_address: value.vault_address,
            offer_denom: value.offer_denom,
            ask_denom: value.ask_denom,
            adapter: value.adapter,
        }
    }
}
