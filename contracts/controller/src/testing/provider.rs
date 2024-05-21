use std::str::FromStr;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response, Timestamp, Uint128,
};
use cw_storage_plus::Item;
use cw_utils::one_coin;
use kujira::{Denom, KujiraMsg, KujiraQuery};
use unstake::{adapter::eris::ExecuteMsg, ContractError};

static PENDING: Item<(Timestamp, Uint128)> = Item::new("pending");

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
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    let rate = Decimal::from_str("1.07375")?;
    match msg {
        ExecuteMsg::WithdrawUnbonded { .. } => {
            let (time, pending) = PENDING.load(deps.storage)?;
            let amount = pending.mul_floor(rate);
            if env.block.time.seconds() - time.seconds() < 14 * 24 * 60 * 60 {
                return Ok(Response::default());
            }

            Ok(Response::default().add_message(Denom::from("quote").send(&info.sender, &amount)))
        }
        ExecuteMsg::QueueUnbond { .. } => {
            let amount = one_coin(&info)?;
            PENDING.save(deps.storage, &(env.block.time, amount.amount))?;

            Ok(Response::default())
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(
    _deps: Deps<KujiraQuery>,
    _env: Env,
    msg: unstake::adapter::eris::ContractQueryMsg,
) -> Result<Binary, ContractError> {
    match msg {
        unstake::adapter::eris::ContractQueryMsg::State {} => Ok(to_json_binary(
            &unstake::adapter::eris::ContractStateResponse {
                exchange_rate: Decimal::from_str("1.07375")?,
            },
        )?),
    }
}
