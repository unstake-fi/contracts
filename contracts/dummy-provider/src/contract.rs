use std::str::FromStr;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Addr, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response, Uint128,
};
use cw_storage_plus::{Item, Map};
use cw_utils::{must_pay, Expiration};
use kujira::{DenomMsg, KujiraMsg, KujiraQuery};
use unstake::{adapter::eris::ExecuteMsg as UnstakeExecuteMsg, ContractError};

use crate::msg::{ExecuteMsg, InstantiateMsg};

const PENDING: Map<Addr, (Expiration, Uint128)> = Map::new("pending");
const INIT: Item<InstantiateMsg> = Item::new("init");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<KujiraQuery>,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    INIT.save(deps.storage, &msg)?;
    let msgs = vec![
        DenomMsg::Create {
            subdenom: msg.base.into(),
        },
        DenomMsg::Create {
            subdenom: msg.lst.into(),
        },
    ];
    Ok(Response::default().add_messages(msgs))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    let rate = Decimal::from_str("1.07375")?;
    let init = INIT.load(deps.storage)?;
    let lst = format!("factory/{}/{}", env.contract.address, init.lst);
    let base = format!("factory/{}/{}", env.contract.address, init.base);
    match msg {
        ExecuteMsg::Execute(UnstakeExecuteMsg::WithdrawUnbonded { .. }) => {
            let (time, pending) = PENDING.load(deps.storage, info.sender.clone())?;
            let amount = pending.mul_floor(rate);
            if !time.is_expired(&env.block) {
                return Ok(Response::default());
            }

            let mint_msg = DenomMsg::Mint {
                denom: base.clone().into(),
                amount,
                recipient: info.sender.clone(),
            };

            Ok(Response::default().add_message(mint_msg))
        }
        ExecuteMsg::Execute(UnstakeExecuteMsg::QueueUnbond { .. }) => {
            let amount = must_pay(&info, &lst)?;
            let time = init.unbond_time.after(&env.block);
            PENDING.save(deps.storage, info.sender, &(time, amount))?;

            Ok(Response::default())
        }
        ExecuteMsg::Mint { denom, amount } => {
            let mint_msg = DenomMsg::Mint {
                denom: format!("factory/{}/{}", env.contract.address, denom).into(),
                amount,
                recipient: info.sender,
            };

            Ok(Response::default().add_message(mint_msg))
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
