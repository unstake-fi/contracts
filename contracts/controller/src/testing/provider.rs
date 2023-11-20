use cosmwasm_schema::cw_serde;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Binary, Deps, DepsMut, Env, MessageInfo, Response};
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
    _msg: ExecuteMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    todo!()
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(
    _deps: Deps<KujiraQuery>,
    _env: Env,
    _msg: unstake::adapter::ContractQueryMsg,
) -> Result<Binary, ContractError> {
    todo!()
}
