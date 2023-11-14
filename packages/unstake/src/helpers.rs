use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{
    instantiate2_address, to_json_binary, Addr, Binary, CodeInfoResponse, Coin, CosmosMsg, Deps,
    Env, StdResult, WasmMsg,
};

use crate::ContractError;

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Delegate(pub Addr);

impl Delegate {
    pub fn addr(&self) -> Addr {
        self.0.clone()
    }

    pub fn call<T: Into<crate::delegate::ExecuteMsg>>(
        &self,
        msg: T,
        funds: Vec<Coin>,
    ) -> Result<CosmosMsg, ContractError> {
        let msg = to_json_binary(&msg.into())?;
        Ok(WasmMsg::Execute {
            contract_addr: self.addr().into(),
            msg,
            funds,
        }
        .into())
    }
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq, JsonSchema)]
pub struct Controller(pub Addr);

impl Controller {
    pub fn addr(&self) -> Addr {
        self.0.clone()
    }

    pub fn call<T: Into<crate::controller::ExecuteMsg>>(
        &self,
        msg: T,
        funds: Vec<Coin>,
    ) -> Result<CosmosMsg, ContractError> {
        let msg = to_json_binary(&msg.into())?;
        Ok(WasmMsg::Execute {
            contract_addr: self.addr().into(),
            msg,
            funds,
        }
        .into())
    }
}

pub fn predict_address(
    code_id: u64,
    label: &String,
    deps: &Deps,
    env: &Env,
) -> StdResult<(Addr, Binary)> {
    let CodeInfoResponse { checksum, .. } = deps.querier.query_wasm_code_info(code_id)?;
    let salt = Binary::from(label.as_bytes().chunks(64).next().unwrap());
    let creator = deps.api.addr_canonicalize(env.contract.address.as_str())?;
    let contract_addr = deps
        .api
        .addr_humanize(&instantiate2_address(&checksum, &creator, &salt).unwrap())?;

    Ok((contract_addr, salt))
}
