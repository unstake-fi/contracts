use cosmwasm_schema::{
    cw_serde,
    serde::{Deserialize, Serialize},
};
use cosmwasm_std::Uint128;
use cw_utils::Duration;
use unstake::adapter::eris::ExecuteMsg as AdapterExecute;

#[cw_serde]
pub struct InstantiateMsg {
    pub unbond_time: Duration,
    pub lst: String,
    pub base: String,
}

#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
#[serde(
    deny_unknown_fields,
    rename_all = "snake_case",
    crate = "cosmwasm_schema::serde"
)]
#[allow(clippy::derive_partial_eq_without_eq)]
pub enum ExecuteMsg {
    Mint {
        denom: String,
        amount: Uint128,
    },
    /// Rewards interfaces
    #[serde(untagged)]
    Execute(AdapterExecute),
}
