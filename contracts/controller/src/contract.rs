#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, CosmosMsg, Deps, DepsMut, Env, MessageInfo, Response, WasmMsg,
};
use cw2::set_contract_version;
use cw_utils::{must_pay, NativeBalance};
use kujira::{amount, Denom};
use unstake::controller::{ExecuteMsg, InstantiateMsg, OfferResponse, QueryMsg};
use unstake::{broker::Broker, ContractError};

use crate::config::Config;

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:unstake";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::from(msg);
    config.save(deps)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    let config = Config::load(deps.as_ref())?;
    match msg {
        ExecuteMsg::Unstake { max_fee } => {
            let amount = must_pay(&info, &config.ask_denom.to_string())?;
            let broker = Broker::load(deps.storage)?;
            let offer = broker.offer(deps.as_ref(), amount)?;
            if offer.fee.gt(&max_fee) {
                return Err(ContractError::MaxFeeExceeded {});
            };
            broker.accept_offer(deps, &offer)?;
            let send_msg = config.offer_denom.send(&info.sender, &offer.amount);
            Ok(Response::default().add_message(send_msg))
        }
        ExecuteMsg::Complete { offer } => {
            // TODO verify calling contract
            let debt_tokens = amount(&config.debt_denom(), info.funds.clone())?;
            let returned_tokens = amount(&config.offer_denom, info.funds)?;
            let mut funds = NativeBalance(vec![
                config.debt_denom().coin(&debt_tokens),
                config.offer_denom.coin(&returned_tokens),
            ]);
            funds.normalize();
            let broker = Broker::load(deps.storage)?;
            broker.close_offer(deps, &offer, debt_tokens, returned_tokens)?;
            let ghost_repay_msg: CosmosMsg = CosmosMsg::Wasm(WasmMsg::Execute {
                contract_addr: config.vault_address.to_string(),
                msg: to_json_binary(&kujira::ghost::receipt_vault::RepayMsg { callback: None })?,
                funds: funds.into_vec(),
            });

            Ok(Response::default().add_message(ghost_repay_msg))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    match msg {
        QueryMsg::Offer { amount } => {
            let denom = Denom::from("TODO");
            let broker = Broker::load(deps.storage)?;
            let offer = broker.offer(deps, amount)?;
            Ok(to_json_binary(&OfferResponse::from(offer))?)
        }
    }
}

#[cfg(test)]
mod tests {}
