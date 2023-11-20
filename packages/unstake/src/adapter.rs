use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{
    Addr, Binary, Coin, CosmosMsg, CustomQuery, Decimal, QuerierWrapper, StdResult, WasmMsg,
};
use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

#[cw_serde]
pub enum Adapter {
    Contract(Contract),
}

#[cw_serde]
pub struct Contract {
    pub address: Addr,
    pub redemption_rate_query: Binary,
    pub unbond_start_msg: Binary,
    pub unbond_end_msg: Binary,
}

impl Contract {
    pub fn redemption_rate<T: CustomQuery>(
        &self,
        querier: QuerierWrapper<T>,
    ) -> StdResult<Decimal> {
        let state: ContractStateResponse =
            querier.query_wasm_smart(self.address.to_string(), &ContractQueryMsg::State {})?;
        Ok(state.exchange_rate)
    }

    pub fn unbond_start<T>(&self, funds: Coin) -> CosmosMsg<T> {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.address.to_string(),
            msg: self.unbond_start_msg.clone(),
            funds: vec![funds],
        })
    }

    pub fn unbond_end<T>(&self) -> CosmosMsg<T> {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.address.to_string(),
            msg: self.unbond_start_msg.clone(),
            funds: vec![],
        })
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
