use std::ops::{AddAssign, Sub};

use crate::config::Config;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, ensure_eq, to_json_binary, Addr, Binary, Coin, CosmosMsg, Deps, DepsMut, Env,
    MessageInfo, Order, Response, StdError, StdResult, Timestamp, Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw_storage_plus::Map;
use cw_utils::{must_pay, NativeBalance};
use kujira::{fee_address, Denom, KujiraMsg, KujiraQuery};
use unstake::controller::{
    DelegatesResponse, ExecuteMsg, InstantiateMsg, OfferResponse, QueryMsg, RatesResponse,
};
use unstake::helpers::{predict_address, Controller};
use unstake::rates::Rates;
use unstake::{broker::Broker, ContractError};

// version info for migration info
const CONTRACT_NAME: &str = "crates.io:unstake";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

static DELEGATES: Map<Addr, Timestamp> = Map::new("delegates");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<KujiraQuery>,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::from(msg.clone());
    config.save(deps.storage)?;
    let broker = Broker::from(msg);
    broker.save(deps.storage)?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    let config = Config::load(deps.storage)?;
    match msg {
        ExecuteMsg::Unstake { max_fee } => {
            let amount = must_pay(&info, config.ask_denom.as_ref())?;
            let broker = Broker::load(deps.storage)?;
            let rates = Rates::load(deps.querier, &config.vault_address)?;
            let offer = broker.offer(deps.as_ref(), &rates, &config.adapter, amount)?;
            if offer.fee.gt(&max_fee) {
                return Err(ContractError::MaxFeeExceeded {});
            };
            broker.accept_offer(deps, &offer)?;
            let send_msg = config.offer_denom.send(&info.sender, &offer.amount);
            let borrow_msg = vault_borrow_msg(&config.vault_address, offer.amount)?;
            let callback_msg = Controller(env.contract.address).call(
                ExecuteMsg::UnstakeCallback {
                    unbond_amount: config.ask_denom.coin(&amount),
                    offer,
                },
                vec![],
            )?;

            Ok(Response::default()
                .add_message(send_msg)
                .add_message(borrow_msg)
                .add_message(callback_msg))
        }
        ExecuteMsg::UnstakeCallback {
            unbond_amount,
            offer,
        } => {
            ensure_eq!(
                info.sender,
                env.contract.address,
                ContractError::Unauthorized {}
            );
            let balances = deps
                .querier
                .query_all_balances(env.contract.address.clone())?;

            let mut funds = NativeBalance(vec![]);

            for Coin { denom, amount } in balances {
                if denom == config.offer_denom.to_string() {
                    funds.add_assign(coin(offer.reserve_allocation.u128(), denom))
                } else {
                    funds.add_assign(coin(amount.u128(), denom))
                }
            }

            let label: String = format!(
                "Unstake.fi delegate {}/{}",
                env.block.height,
                env.transaction
                    .as_ref()
                    .map(|x| x.index)
                    .unwrap_or_default()
            );

            let (address, salt) =
                predict_address(config.delegate_code_id, &label, &deps.as_ref(), &env)?;

            let msg = unstake::delegate::InstantiateMsg {
                unbond_amount,
                controller: env.contract.address.clone(),
                offer: offer.clone(),
                adapter: config.adapter,
            };

            let instantiate: WasmMsg = WasmMsg::Instantiate2 {
                admin: Some(env.contract.address.into()),
                code_id: config.delegate_code_id,
                label,
                msg: to_json_binary(&msg)?,
                funds: funds.into_vec(),
                salt,
            };

            DELEGATES.save(deps.storage, address, &env.block.time)?;

            Ok(Response::default().add_message(instantiate))
        }
        ExecuteMsg::Complete { offer } => {
            DELEGATES
                .load(deps.storage, info.sender.clone())
                .map_err(|_| ContractError::Unauthorized {})?;
            DELEGATES.remove(deps.storage, info.sender);

            let debt_tokens = amount(&config.debt_denom(), info.funds.clone())?;
            let returned_tokens = amount(&config.offer_denom, info.funds)?;
            let rates = Rates::load(deps.querier, &config.vault_address)?;
            // We'll always get the reserve allocation back. If we get nothing else back it means the
            // unbonding hasn't yet completed
            if returned_tokens.sub(offer.reserve_allocation).is_zero() {
                return Err(ContractError::InsufficentFunds {});
            }
            let broker = Broker::load(deps.storage)?;

            // Calculate how much we need to send back to Ghost. Could be more or less than the offer amount
            let (repay_amount, protocol_fee_amount) = broker.close_offer(
                deps,
                &rates,
                &offer,
                debt_tokens,
                returned_tokens,
                config.protocol_fee,
            )?;

            let mut funds = NativeBalance(vec![
                config.debt_denom().coin(&debt_tokens),
                config.offer_denom.coin(&repay_amount),
            ]);
            funds.normalize();

            let ghost_repay_msg = vault_repay_msg(&config.vault_address, funds.into_vec())?;

            let mut msgs = vec![ghost_repay_msg];
            if !protocol_fee_amount.is_zero() {
                msgs.push(
                    config
                        .offer_denom
                        .send(&fee_address(), &protocol_fee_amount),
                )
            }

            Ok(Response::default().add_messages(msgs))
        }
        ExecuteMsg::Fund {} => {
            let amount = must_pay(&info, config.offer_denom.as_ref())?;
            Broker::fund_reserves(deps.storage, amount)?;
            Ok(Response::default())
        }
        ExecuteMsg::UpdateBroker { min_rate, duration } => {
            ensure_eq!(info.sender, config.owner, ContractError::Unauthorized {});
            let mut broker = Broker::load(deps.storage)?;
            broker.update(min_rate, duration);
            broker.save(deps.storage)?;
            Ok(Response::default())
        }
        ExecuteMsg::UpdateConfig {
            owner,
            protocol_fee,
            delegate_code_id,
        } => {
            ensure_eq!(info.sender, config.owner, ContractError::Unauthorized {});
            let mut config = Config::load(deps.storage)?;
            config.update(owner, protocol_fee, delegate_code_id);
            config.save(deps.storage)?;
            Ok(Response::default())
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<KujiraQuery>, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    let config = Config::load(deps.storage)?;
    let rates = Rates::load(deps.querier, &config.vault_address)?;
    match msg {
        QueryMsg::Offer { amount } => {
            let broker = Broker::load(deps.storage)?;
            let offer = broker.offer(deps, &rates, &config.adapter, amount)?;
            Ok(to_json_binary(&OfferResponse::from(offer))?)
        }
        QueryMsg::Delegates {} => {
            let delegates = DELEGATES
                .range(deps.storage, None, None, Order::Ascending)
                .collect::<StdResult<Vec<(Addr, Timestamp)>>>()?;
            let response = DelegatesResponse { delegates };
            Ok(to_json_binary(&response)?)
        }
        QueryMsg::Rates {} => Ok(to_json_binary(&RatesResponse::from(rates))?),
    }
}

pub fn vault_borrow_msg<T>(addr: &Addr, amount: Uint128) -> StdResult<CosmosMsg<T>> {
    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: addr.to_string(),
        msg: to_json_binary(&kujira_ghost::receipt_vault::ExecuteMsg::Borrow(
            kujira_ghost::receipt_vault::BorrowMsg {
                amount,
                callback: None,
            },
        ))?,
        funds: vec![],
    }))
}

pub fn vault_repay_msg<T>(addr: &Addr, coins: Vec<Coin>) -> StdResult<CosmosMsg<T>> {
    Ok(CosmosMsg::Wasm(WasmMsg::Execute {
        contract_addr: addr.to_string(),
        msg: to_json_binary(&kujira_ghost::receipt_vault::ExecuteMsg::Repay(
            kujira_ghost::receipt_vault::RepayMsg { callback: None },
        ))?,
        funds: coins,
    }))
}

pub fn amount(denom: &Denom, funds: Vec<Coin>) -> StdResult<Uint128> {
    let coin = funds
        .iter()
        .find(|d| &Denom::from(d.denom.clone()) == denom);
    match coin {
        None => Err(StdError::not_found(denom.to_string())),
        Some(Coin { amount, .. }) => Ok(*amount),
    }
}
