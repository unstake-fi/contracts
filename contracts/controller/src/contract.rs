#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{to_json_binary, Binary, Deps, DepsMut, Env, MessageInfo, Response};
use cw2::set_contract_version;
use cw_utils::{must_pay, one_coin, NativeBalance};
use kujira::{amount, Denom};
use unstake::controller::{ExecuteMsg, InstantiateMsg, OfferResponse, QueryMsg};
use unstake::{broker::Broker, ContractError};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:unstake";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut,
    _env: Env,
    _info: MessageInfo,
    _msg: InstantiateMsg,
) -> Result<Response, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    unimplemented!()
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut,
    _env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response, ContractError> {
    match msg {
        ExecuteMsg::Unstake { max_fee } => {
            let denom = Denom::from("TODO");
            let amount = must_pay(&info, &denom.to_string())?;
            let broker = Broker::load(deps.storage)?;
            let offer = broker.offer(deps.as_ref(), amount)?;
            if offer.fee.gt(&max_fee) {
                return Err(ContractError::MaxFeeExceeded {});
            };
            broker.accept_offer(deps, &offer)?;
            let send_msg = denom.send(&info.sender, &offer.amount);
            Ok(Response::default().add_message(send_msg))
        }
        ExecuteMsg::Complete { offer } => {
            // TODO verify calling contract
            let debt_tokens = amount(&Denom::from("TODO: debt_denom"), info.funds.clone())?;
            let returned_tokens = amount(&Denom::from("TODO: quote_denom"), info.funds)?;
            let broker = Broker::load(deps.storage)?;
            broker.close_offer(deps, &offer, debt_tokens, returned_tokens)?;

            Ok(Response::default())
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
