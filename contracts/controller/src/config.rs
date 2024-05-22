use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Decimal, StdResult, Storage};
use cw_storage_plus::Item;
use kujira_ghost::receipt_vault::ConfigResponse as GhostConfig;
use monetary::Denom;
use unstake::{
    adapter::Adapter,
    controller::{ConfigResponse, InstantiateMsg},
    denoms::{Ask, Base, Debt, Rcpt},
};

static CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub protocol_fee: Decimal,
    pub protocol_fee_address: Addr,
    pub delegate_code_id: u64,
    pub reserve_address: Addr,
    pub vault_address: Addr,
    pub offer_denom: Denom<Base>,
    pub ask_denom: Denom<Ask>,
    pub debt_denom: Denom<Debt>,
    pub ghost_denom: Denom<Rcpt>,
    pub adapter: Adapter,
}

impl Config {
    pub fn new(msg: InstantiateMsg, ghost_cfg: GhostConfig) -> Self {
        Self {
            owner: msg.owner,
            protocol_fee: msg.protocol_fee,
            protocol_fee_address: msg.protocol_fee_address,
            delegate_code_id: msg.delegate_code_id,
            reserve_address: msg.reserve_address,
            vault_address: msg.vault_address,
            offer_denom: msg.offer_denom,
            ask_denom: msg.ask_denom,
            debt_denom: Denom::new(ghost_cfg.debt_token_denom),
            ghost_denom: Denom::new(ghost_cfg.receipt_denom),
            adapter: msg.adapter,
        }
    }
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
}

impl From<Config> for ConfigResponse {
    fn from(value: Config) -> Self {
        Self {
            owner: value.owner,
            protocol_fee: value.protocol_fee,
            protocol_fee_address: value.protocol_fee_address,
            delegate_code_id: value.delegate_code_id,
            reserve_address: value.reserve_address,
            vault_address: value.vault_address,
            offer_denom: value.offer_denom,
            ask_denom: value.ask_denom,
            debt_denom: value.debt_denom,
            ghost_denom: value.ghost_denom,
            adapter: value.adapter,
        }
    }
}
