use std::str::FromStr;

use cosmwasm_schema::cw_serde;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response, Uint128,
};
use cw_storage_plus::Item;
use cw_utils::one_coin;
use kujira::{Denom, KujiraMsg, KujiraQuery};
use unstake::ContractError;

static PENDING: Item<Uint128> = Item::new("pending");

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
    deps: DepsMut<KujiraQuery>,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    let rate = Decimal::from_str("1.07375")?;
    match msg {
        ExecuteMsg::WithdrawUnbonded {} => {
            let pending = PENDING.load(deps.storage)?;
            let amount = pending * rate;

            Ok(Response::default().add_message(Denom::from("quote").send(&info.sender, &amount)))
        }
        ExecuteMsg::QueueUnbond {} => {
            let amount = one_coin(&info)?;
            PENDING.save(deps.storage, &amount.amount)?;

            Ok(Response::default())
        }
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
