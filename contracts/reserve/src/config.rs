use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Env, QuerierWrapper, StdResult, Storage};
use cw_storage_plus::Item;
use kujira::KujiraQuery;
use monetary::Denom;
use unstake::{
    denoms::{Base, Rcpt, Rsv},
    reserve::{ConfigResponse, InstantiateMsg},
};

use kujira_ghost::receipt_vault::{
    ConfigResponse as GhostConfigResponse, QueryMsg as GhostQueryMsg,
};

use crate::contract::URSV;

pub const CONFIG: Item<Config> = Item::new("config");

#[cw_serde]
pub struct Config {
    pub owner: Addr,
    pub base_denom: Denom<Base>,
    pub rsv_denom: Denom<Rsv>,
    pub ghost_denom: Denom<Rcpt>,
    pub ghost_vault_addr: Addr,
}

impl Config {
    pub fn new(
        msg: InstantiateMsg,
        querier: &QuerierWrapper<KujiraQuery>,
        env: &Env,
    ) -> StdResult<Self> {
        let vault_cfg: GhostConfigResponse =
            querier.query_wasm_smart(&msg.ghost_vault_addr, &GhostQueryMsg::Config {})?;
        let rsv_denom = Denom::new(format!("factory/{}/{}", env.contract.address, URSV));
        let ghost_denom = Denom::new(vault_cfg.receipt_denom);
        Ok(Self {
            owner: msg.owner,
            base_denom: msg.base_denom,
            rsv_denom,
            ghost_denom,
            ghost_vault_addr: msg.ghost_vault_addr,
        })
    }
    pub fn load(storage: &dyn Storage) -> StdResult<Self> {
        CONFIG.load(storage)
    }

    pub fn save(&self, storage: &mut dyn Storage) -> StdResult<()> {
        CONFIG.save(storage, self)
    }

    pub fn update(&mut self, owner: Option<Addr>) {
        if let Some(owner) = owner {
            self.owner = owner
        }
    }
}

impl From<Config> for ConfigResponse {
    fn from(value: Config) -> Self {
        Self {
            owner: value.owner,
            base_denom: value.base_denom,
            rsv_denom: value.rsv_denom,
            ghost_denom: value.ghost_denom,
            ghost_vault_addr: value.ghost_vault_addr,
        }
    }
}
