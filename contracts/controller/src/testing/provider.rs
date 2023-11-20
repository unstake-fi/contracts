use std::str::FromStr;

use cosmwasm_schema::cw_serde;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response};
use kujira::{KujiraMsg, KujiraQuery};
use unstake::ContractError;

#[cw_serde]
pub enum ExecuteMsg {
    WithdrawUnbonded {},
    QueueUnbond {},
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    _deps: DepsMut<KujiraQuery>,
    _env: Env,
    _info: MessageInfo,
    _msg: (),
) -> Result<Response<KujiraMsg>, ContractError> {
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    _deps: DepsMut<KujiraQuery>,
    _env: Env,
    _info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    match msg {
        ExecuteMsg::WithdrawUnbonded {} => Ok(Response::default()),
        ExecuteMsg::QueueUnbond {} => Ok(Response::default()),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(
    _deps: Deps<KujiraQuery>,
    _env: Env,
    msg: unstake::adapter::ContractQueryMsg,
) -> Result<Binary, ContractError> {
    match msg {
        unstake::adapter::ContractQueryMsg::State {} => {
            Ok(to_json_binary(&unstake::adapter::ContractStateResponse {
                exchange_rate: Decimal::from_str("1.07375")?,
            })?)
        }
    }
}
