use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Binary, Coin, CosmosMsg, WasmMsg};

#[cw_serde]
pub enum Adapter {
    Contract(Contract),
}

#[cw_serde]
pub struct Contract {
    pub address: Addr,
    pub unbond_start_msg: Binary,
    pub unbond_end_msg: Binary,
}

impl Contract {
    pub fn unbond_start(&self, funds: Vec<Coin>) -> CosmosMsg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.address.to_string(),
            msg: self.unbond_start_msg.clone(),
            funds,
        })
    }

    pub fn unbond_end(&self) -> CosmosMsg {
        CosmosMsg::Wasm(WasmMsg::Execute {
            contract_addr: self.address.to_string(),
            msg: self.unbond_start_msg.clone(),
            funds: vec![],
        })
    }
}
