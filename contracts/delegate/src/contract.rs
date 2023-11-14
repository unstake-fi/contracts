#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{Addr, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response};
use cw_storage_plus::Item;
use unstake::broker::Offer;
use unstake::delegate::{ExecuteMsg, InstantiateMsg, QueryMsg};
use unstake::helpers::{Controller, Delegate};
use unstake::ContractError;

static CONTROLLER: Item<Addr> = Item::new("controller");
static OFFER: Item<Offer> = Item::new("offer");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    CONTROLLER.save(deps.storage, &msg.controller)?;
    OFFER.save(deps.storage, &msg.offer)?;
    let unbond_msg: CosmosMsg = todo!();
    Ok(Response::default().add_message(unbond_msg))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Complete {} => {
            let claim_msg: CosmosMsg = todo!();
            let callback_msg =
                Delegate(env.contract.address).call(ExecuteMsg::Callback {}, vec![])?;
            Ok(Response::default()
                .add_message(claim_msg)
                .add_message(callback_msg))
        }
        ExecuteMsg::Callback {} => {
            let funds = deps.querier.query_all_balances(env.contract.address)?;
            let offer = OFFER.load(deps.storage)?;
            let controller_msg = Controller(CONTROLLER.load(deps.storage)?)
                .call(unstake::controller::ExecuteMsg::Complete { offer }, funds)?;
            Ok(Response::default().add_message(controller_msg))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {}
}

#[cfg(test)]
mod tests {}
