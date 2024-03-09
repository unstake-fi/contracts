use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{
    to_json_binary, Addr, Coin, CosmosMsg, CustomQuery, Decimal, QuerierWrapper, StdResult, WasmMsg,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::interface::Unstake;
#[cw_serde]
pub struct Gravedigger(Addr);

impl Unstake for Gravedigger {
    fn redemption_rate<T: CustomQuery>(&self, querier: QuerierWrapper<T>) -> StdResult<Decimal> {
        let state: ContractStateResponse =
            querier.query_wasm_smart(self.0.to_string(), &ContractQueryMsg::State {})?;
        Ok(state.exchange_rate)
    }

    fn unbond_start<T>(&self, funds: Coin) -> CosmosMsg<T> {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.0.to_string(),
            msg: to_json_binary(&ExecuteMsg::Unbond { receiver: None }).unwrap(),
            funds: vec![funds],
        })
    }

    fn unbond_end<T>(&self) -> CosmosMsg<T> {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.0.to_string(),
            msg: to_json_binary(&ExecuteMsg::WithdrawUnbonded { receiver: None }).unwrap(),
            funds: vec![],
        })
    }
}

impl From<Addr> for Gravedigger {
    fn from(value: Addr) -> Self {
        Self(value)
    }
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum ContractQueryMsg {
    /// The contract's current state. Response: `StateResponse`
    #[returns(ContractStateResponse)]
    State {},
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, JsonSchema)]
pub struct ContractStateResponse {
    /// The exchange rate between ustake and utoken, in terms of utoken per ustake
    pub exchange_rate: Decimal,
}

#[cw_serde]
pub enum ExecuteMsg {
    WithdrawUnbonded { receiver: Option<String> },
    Unbond { receiver: Option<String> },
}
