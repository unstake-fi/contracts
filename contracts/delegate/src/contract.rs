#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure_eq, Addr, Binary, Coins, Deps, DepsMut, Env, Event, MessageInfo, Response,
};
use cw_storage_plus::Item;
use kujira::{KujiraMsg, KujiraQuery};
use unstake::adapter::Adapter;
use unstake::broker::Offer;
use unstake::delegate::{ExecuteMsg, InstantiateMsg, QueryMsg};
use unstake::helpers::{Controller, Delegate};
use unstake::ContractError;

static CONTROLLER: Item<Addr> = Item::new("controller");
static OFFER: Item<Offer> = Item::new("offer");
static ADAPTER: Item<Adapter> = Item::new("adapter");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<KujiraQuery>,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    CONTROLLER.save(deps.storage, &msg.controller)?;
    OFFER.save(deps.storage, &msg.offer)?;
    ADAPTER.save(deps.storage, &msg.adapter)?;
    match msg.adapter {
        Adapter::Contract(c) => {
            let unbond_msg = c.unbond_start(msg.unbond_amount);
            Ok(Response::default().add_message(unbond_msg))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    match msg {
        ExecuteMsg::Complete {} => {
            let adapter = ADAPTER.load(deps.storage)?;

            match adapter {
                Adapter::Contract(c) => {
                    let claim_msg = c.unbond_end();
                    let callback_msg =
                        Delegate(env.contract.address).call(ExecuteMsg::Callback {}, vec![])?;

                    Ok(Response::default()
                        .add_message(claim_msg)
                        .add_message(callback_msg))
                }
            }
        }
        ExecuteMsg::Callback {} => {
            ensure_eq!(
                info.sender,
                env.contract.address,
                ContractError::Unauthorized {}
            );
            let funds = deps.querier.query_all_balances(env.contract.address)?;
            let offer = OFFER.load(deps.storage)?;
            let controller_msg = Controller(CONTROLLER.load(deps.storage)?).call(
                unstake::controller::ExecuteMsg::Complete {
                    offer: offer.clone(),
                },
                funds.clone(),
            )?;
            let event: Event = Event::new("unstake/delegate/callback")
                .add_attribute("offer", offer)
                .add_attribute(
                    "funds",
                    Coins::try_from(funds).unwrap_or_default().to_string(),
                );

            Ok(Response::default()
                .add_event(event)
                .add_message(controller_msg))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps<KujiraQuery>, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {}
}

#[cfg(test)]
mod tests {}
