use std::ops::{AddAssign, Sub};

use crate::config::Config;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coin, ensure_eq, to_json_binary, wasm_execute, Addr, Binary, Coin, CosmosMsg, Decimal, Deps,
    DepsMut, Empty, Env, Event, MessageInfo, Order, Response, StdError, StdResult, Timestamp,
    Uint128, WasmMsg,
};
use cw2::set_contract_version;
use cw_storage_plus::Map;
use cw_utils::{must_pay, NativeBalance};
use kujira::{Denom, DenomMsg, KujiraMsg, KujiraQuery};
use serde::Serialize;
use unstake::broker::Status;
use unstake::controller::{
    CallbackType, ConfigResponse, DelegatesResponse, ExecuteMsg, InstantiateMsg, OfferResponse,
    QueryMsg, RatesResponse, StatusResponse,
};
use unstake::helpers::predict_address;
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
    let create_msg: CosmosMsg<KujiraMsg> = DenomMsg::Create {
        subdenom: "ursv".into(),
    }
    .into();
    Ok(Response::default().add_message(create_msg))
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
        ExecuteMsg::Unstake { max_fee, callback } => {
            let amount = must_pay(&info, config.ask_denom.as_ref())?;
            let broker = Broker::load(deps.storage)?;
            let rates = Rates::load(deps.querier, &config.adapter, &config.vault_address)?;
            let offer = broker.offer(deps.as_ref(), &rates, amount)?;
            if offer.fee.gt(&max_fee) {
                return Err(ContractError::MaxFeeExceeded {});
            };
            broker.accept_offer(deps, &offer)?;
            let send_msg = callback
                .map(|cb| cb.to_message(&info.sender, Empty {}, []).unwrap())
                .unwrap_or(config.offer_denom.send(&info.sender, &offer.offer_amount));
            let borrow_msg = vault_borrow_msg(
                &config.vault_address,
                offer.offer_amount,
                Some(&CallbackType::Unstake {
                    offer: offer.clone(),
                    unbond_amount: config.ask_denom.coin(&amount),
                }),
            )?;

            let event = Event::new("unstake/controller/unstake")
                .add_attribute("amount", amount)
                .add_attribute("rates", rates)
                .add_attribute("offer", offer);

            Ok(Response::default()
                .add_event(event)
                .add_message(borrow_msg)
                .add_message(send_msg))
        }
        ExecuteMsg::Callback(cb) => {
            let cb_type: CallbackType = cb.deserialize_callback()?;
            match cb_type {
                CallbackType::Unstake {
                    offer,
                    unbond_amount,
                } => {
                    ensure_eq!(
                        info.sender,
                        config.vault_address,
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
                        unbond_amount: unbond_amount.clone(),
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

                    DELEGATES.save(deps.storage, address.clone(), &env.block.time)?;

                    let event: Event = Event::new("unstake/controller/callback/unstake")
                        .add_attribute("unbond_amount", unbond_amount.amount)
                        .add_attribute("delegate", address);

                    Ok(Response::default()
                        .add_event(event)
                        .add_message(instantiate))
                }
            }
        }
        ExecuteMsg::Complete { offer } => {
            DELEGATES
                .load(deps.storage, info.sender.clone())
                .map_err(|_| ContractError::Unauthorized {})?;
            DELEGATES.remove(deps.storage, info.sender);

            let debt_tokens = amount(&config.debt_denom(), info.funds.clone())?;
            let returned_tokens = amount(&config.offer_denom, info.funds)?;
            let rates = Rates::load(deps.querier, &config.adapter, &config.vault_address)?;
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
                        .send(&config.protocol_fee_address, &protocol_fee_amount),
                )
            }

            let event: Event = Event::new("unstake/controller/complete")
                .add_attribute("returned_tokens", returned_tokens)
                .add_attribute("repay_amount", repay_amount)
                .add_attribute("protocol_fee_amount", protocol_fee_amount);

            Ok(Response::default().add_event(event).add_messages(msgs))
        }
        ExecuteMsg::Fund {} => {
            let amount = must_pay(&info, config.offer_denom.as_ref())?;
            let reserve_share = Broker::fund_reserves(deps.storage, amount)?;
            let supply = deps.querier.query_supply(token_denom(&env).to_string())?;
            let mint_amount = reserve_share
                .map(|x| supply.amount * x)
                .unwrap_or_else(|| amount);
            let mint_msg: CosmosMsg<KujiraMsg> = DenomMsg::Mint {
                denom: token_denom(&env),
                amount: mint_amount,
                recipient: info.sender,
            }
            .into();
            Ok(Response::default().add_message(mint_msg))
        }
        ExecuteMsg::Withdraw {} => {
            let amount = must_pay(&info, token_denom(&env).to_string().as_str())?;
            let supply = deps.querier.query_supply(token_denom(&env).to_string())?;
            let share = Decimal::from_ratio(amount, supply.amount);
            let removed_amount = Broker::withdraw_reserves(deps.storage, share)?;
            let burn_msg: CosmosMsg<KujiraMsg> = DenomMsg::Burn {
                denom: token_denom(&env),
                amount,
            }
            .into();
            let send_msg: CosmosMsg<KujiraMsg> =
                config.offer_denom.send(&info.sender, &removed_amount);

            Ok(Response::default()
                .add_message(burn_msg)
                .add_message(send_msg))
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
            protocol_fee_address,
            delegate_code_id,
        } => {
            ensure_eq!(info.sender, config.owner, ContractError::Unauthorized {});
            let mut config = Config::load(deps.storage)?;
            config.update(owner, protocol_fee, protocol_fee_address, delegate_code_id);
            config.save(deps.storage)?;
            Ok(Response::default())
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<KujiraQuery>, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    let config = Config::load(deps.storage)?;
    let rates = Rates::load(deps.querier, &config.adapter, &config.vault_address)?;
    match msg {
        QueryMsg::Offer { amount } => {
            let broker = Broker::load(deps.storage)?;
            let offer = broker.offer(deps, &rates, amount)?;
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
        QueryMsg::Config {} => Ok(to_json_binary(&ConfigResponse::from(config))?),
        QueryMsg::Status {} => Ok(to_json_binary(&StatusResponse::from(Status::load(
            deps.storage,
        )))?),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(
    _deps: DepsMut<KujiraQuery>,
    env: Env,
    msg: Vec<(Addr, Uint128)>,
) -> StdResult<Response<KujiraMsg>> {
    let create_msg: CosmosMsg<KujiraMsg> = DenomMsg::Create {
        subdenom: "ursv".into(),
    }
    .into();
    let send_msgs = msg
        .into_iter()
        .map(|(addr, amount)| DenomMsg::Mint {
            denom: format!("factory/{}/ursv", env.contract.address).into(),
            amount,
            recipient: addr,
        })
        .collect::<Vec<_>>();
    Ok(Response::default()
        .add_message(create_msg)
        .add_messages(send_msgs))
}

pub fn vault_borrow_msg<T>(
    addr: &Addr,
    amount: Uint128,
    callback: Option<&impl Serialize>,
) -> StdResult<CosmosMsg<T>> {
    wasm_execute(
        addr,
        &kujira_ghost::receipt_vault::ExecuteMsg::Borrow(kujira_ghost::receipt_vault::BorrowMsg {
            amount,
            callback: callback
                .map(|c| to_json_binary(c).map(Into::into))
                .transpose()?,
        }),
        vec![],
    )
    .map(Into::into)
}

pub fn vault_repay_msg<T>(addr: &Addr, coins: Vec<Coin>) -> StdResult<CosmosMsg<T>> {
    wasm_execute(
        addr,
        &kujira_ghost::receipt_vault::ExecuteMsg::Repay(kujira_ghost::receipt_vault::RepayMsg {
            callback: None,
        }),
        coins,
    )
    .map(Into::into)
}

pub fn amount(denom: &Denom, funds: Vec<Coin>) -> StdResult<Uint128> {
    let coin = funds.iter().find(|d| d.denom == denom.to_string());
    match coin {
        None => Err(StdError::not_found(denom.to_string())),
        Some(Coin { amount, .. }) => Ok(*amount),
    }
}

fn token_denom(env: &Env) -> Denom {
    let addr = env.contract.address.clone();
    format!("factory/{addr}/ursv").into()
}
