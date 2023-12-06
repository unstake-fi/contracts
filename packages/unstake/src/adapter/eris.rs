use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{
    to_json_binary, Addr, Coin, CosmosMsg, CustomQuery, Decimal, QuerierWrapper, StdResult, WasmMsg,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use super::adapter::Unstake;
#[cw_serde]
pub struct Eris(Addr);

impl Unstake for Eris {
    fn redemption_rate<T: CustomQuery>(&self, querier: QuerierWrapper<T>) -> StdResult<Decimal> {
        let state: ContractStateResponse =
            querier.query_wasm_smart(self.0.to_string(), &ContractQueryMsg::State {})?;
        Ok(state.exchange_rate)
    }

    fn unbond_start<T>(&self, funds: Coin) -> CosmosMsg<T> {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.0.to_string(),
            msg: to_json_binary(&ExecuteMsg::QueueUnbond { receiver: None }).unwrap(),
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

impl From<Addr> for Eris {
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
    QueueUnbond { receiver: Option<String> },
}
