use schemars::JsonSchema;
use serde::{Deserialize, Serialize};

use cosmwasm_std::{to_json_binary, Addr, Coin, CosmosMsg, WasmMsg};

use crate::ContractError;

/// Delegate is a wrapper around Addr that provides a lot of helpers
/// for working with this.
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

/// Controller is a wrapper around Addr that provides a lot of helpers
/// for working with this.
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
