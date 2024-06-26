use std::ops::Sub;

use crate::config::Config;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure_eq, to_json_binary, wasm_execute, Addr, Binary, Coin, CosmosMsg, Deps, DepsMut, Empty,
    Env, Event, MessageInfo, Order, Response, StdError, StdResult, Timestamp, WasmMsg,
};
use cw2::set_contract_version;
use cw_storage_plus::Map;
use cw_utils::NativeBalance;
use kujira::{KujiraMsg, KujiraQuery};
use monetary::{must_pay, AmountU128, CheckedCoin, Denom};
use serde::Serialize;
use unstake::broker::Status;
use unstake::controller::{
    CallbackType, DelegatesResponse, ExecuteMsg, InstantiateMsg, OfferResponse, QueryMsg,
    RatesResponse, StatusResponse,
};
use unstake::denoms::Base;
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
    let ghost_cfg: kujira_ghost::receipt_vault::ConfigResponse = deps.querier.query_wasm_smart(
        &msg.vault_address,
        &kujira_ghost::receipt_vault::QueryMsg::Config {},
    )?;
    let config = Config::new(msg.clone(), ghost_cfg);
    config.save(deps.storage)?;
    let broker = Broker::from(msg);
    broker.init(deps.storage)?;
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
        ExecuteMsg::Unstake { max_fee, callback } => {
            let amount = must_pay(&info, &config.ask_denom)?;
            let broker = Broker::load(deps.storage)?;
            let rates = Rates::load(deps.querier, &config.adapter, &config.vault_address)?;
            let reserve_status = deps.querier.query_wasm_smart(
                &config.reserve_address,
                &unstake::reserve::QueryMsg::Status {},
            )?;
            let offer = broker.offer(&reserve_status, &rates, amount)?;
            if offer.fee.gt(&max_fee) {
                return Err(ContractError::MaxFeeExceeded {});
            };
            broker.accept_offer(deps.storage, &offer)?;

            let borrow_amount = offer.offer_amount - offer.reserve_allocation;

            let mut msgs = vec![];
            // Number one, request reserves from the reserve contract.
            if !offer.reserve_allocation.is_zero() {
                msgs.push(request_reserve_msg(
                    &config.reserve_address,
                    offer.reserve_allocation,
                )?);
            }
            // Number two, borrow from GHOST
            msgs.push(vault_borrow_msg(
                &config.vault_address,
                borrow_amount,
                Some(&CallbackType::GhostBorrow {
                    offer: offer.clone(),
                }),
            )?);
            // Number three, return instant liquidity to sender.
            msgs.push(
                callback
                    .map(|cb| cb.to_message(&info.sender, Empty {}, []).unwrap())
                    .unwrap_or(
                        config
                            .offer_denom
                            .send(&info.sender, offer.offer_amount)
                            .into(),
                    ),
            );

            // Calculate delegate address in advance
            let label = delegate_label(&env);
            let (address, _) =
                predict_address(config.delegate_code_id, &label, &deps.as_ref(), &env)?;

            let event = Event::new("unstake/controller/unstake")
                .add_attribute("amount", amount)
                .add_attribute("rates", rates)
                .add_attribute("offer", offer)
                .add_attribute("sender", info.sender)
                .add_attribute("delegate", address);

            Ok(Response::default().add_event(event).add_messages(msgs))
        }
        ExecuteMsg::Callback(cb) => {
            let cb_type: CallbackType = cb.deserialize_callback()?;
            let offer = match cb_type {
                CallbackType::GhostBorrow { offer } => offer,
            };

            ensure_eq!(
                info.sender,
                config.vault_address,
                ContractError::Unauthorized {}
            );
            let debt = amount(&config.debt_denom, &info.funds)?;
            let debt_amount = debt.amount;
            let unbond = config.ask_denom.coin(offer.unbond_amount);
            let mut funds = NativeBalance(vec![debt.into(), unbond.into()]);
            funds.normalize();

            let label = delegate_label(&env);
            let (address, salt) =
                predict_address(config.delegate_code_id, &label, &deps.as_ref(), &env)?;

            let msg = unstake::delegate::InstantiateMsg {
                unbond_amount: config.ask_denom.coin(offer.unbond_amount),
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
                .add_attribute("unbond_amount", offer.unbond_amount)
                .add_attribute("debt_amount", debt_amount)
                .add_attribute("delegate", address);

            Ok(Response::default()
                .add_event(event)
                .add_message(instantiate))
        }
        ExecuteMsg::Complete { offer } => {
            DELEGATES
                .load(deps.storage, info.sender.clone())
                .map_err(|_| ContractError::Unauthorized {})?;
            DELEGATES.remove(deps.storage, info.sender.clone());

            let debt = amount(&config.debt_denom, &info.funds)?;
            let base = amount(&config.offer_denom, &info.funds)?;

            let rates = Rates::load(deps.querier, &config.adapter, &config.vault_address)?;
            let broker = Broker::load(deps.storage)?;

            let mut msgs = vec![];
            // We'll always get the reserve allocation back. If we get nothing else back it means the
            // unbonding hasn't yet completed
            if base.amount.sub(offer.reserve_allocation).is_zero() {
                return Err(ContractError::InsufficentFunds {});
            }

            let (repay_funds, reserve_return, base_fee_amount) =
                broker.close_offer(deps, &rates, &offer, debt, base.clone())?;
            let protocol_fee = base_fee_amount.dec_mul_floor(config.protocol_fee);
            let reserve_fee = base_fee_amount.sub(protocol_fee);

            // repay ghost
            let ghost_repay_msg = vault_repay_msg(&config.vault_address, repay_funds.clone())?;
            msgs.push(ghost_repay_msg);

            // return reserves, with any fees
            let reserve_return_amount = reserve_return + reserve_fee;
            if !reserve_return_amount.is_zero() {
                let reserve_repay_msg =
                    repay_reserve_msg(&config, offer.reserve_allocation, reserve_return_amount)?;
                msgs.push(reserve_repay_msg);
            }

            // Finally, send the protocol fee to the fee address
            if !protocol_fee.is_zero() {
                msgs.push(
                    config
                        .offer_denom
                        .send(&config.protocol_fee_address, protocol_fee)
                        .into(),
                );
            }
            let event: Event = Event::new("unstake/controller/complete")
                .add_attribute("returned_tokens", base.amount)
                .add_attribute(
                    "repay_amount",
                    repay_funds
                        .iter()
                        .map(|c| c.to_string())
                        .collect::<Vec<String>>()
                        .join(", "),
                )
                .add_attribute("protocol_fee_amount", protocol_fee)
                .add_attribute("reserve_fee", reserve_fee)
                .add_attribute("delegate", info.sender);
            Ok(Response::default().add_event(event).add_messages(msgs))
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
    let broker = Broker::load(deps.storage)?;
    match msg {
        QueryMsg::Offer { amount } => {
            let reserve_status = deps.querier.query_wasm_smart(
                &config.reserve_address,
                &unstake::reserve::QueryMsg::Status {},
            )?;
            let offer = broker.offer(&reserve_status, &rates, amount)?;
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
        QueryMsg::Config {} => Ok(to_json_binary(&config.to_response(broker))?),
        QueryMsg::Status {} => Ok(to_json_binary(&StatusResponse::from(Status::load(
            deps.storage,
        )))?),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(deps: DepsMut<KujiraQuery>, _env: Env, _msg: ()) -> StdResult<Response<KujiraMsg>> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;

    Ok(Response::default())
}

pub fn vault_borrow_msg<T>(
    addr: &Addr,
    amount: AmountU128<Base>,
    callback: Option<&impl Serialize>,
) -> StdResult<CosmosMsg<T>> {
    wasm_execute(
        addr,
        &kujira_ghost::receipt_vault::ExecuteMsg::Borrow(kujira_ghost::receipt_vault::BorrowMsg {
            amount: amount.uint128(),
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

pub fn request_reserve_msg<T>(addr: &Addr, amount: AmountU128<Base>) -> StdResult<CosmosMsg<T>> {
    wasm_execute(
        addr,
        &unstake::reserve::ExecuteMsg::RequestReserves {
            requested_amount: amount,
            callback: None,
        },
        vec![],
    )
    .map(Into::into)
}

pub fn repay_reserve_msg<T>(
    config: &Config,
    original_amount: AmountU128<Base>,
    return_amount: AmountU128<Base>,
) -> StdResult<CosmosMsg<T>> {
    wasm_execute(
        &config.reserve_address,
        &unstake::reserve::ExecuteMsg::ReturnReserves {
            original_amount,
            callback: None,
        },
        vec![config.offer_denom.coin(return_amount).into()],
    )
    .map(Into::into)
}

pub fn amount<T>(denom: &Denom<T>, funds: &[Coin]) -> StdResult<CheckedCoin<T>> {
    let coin = funds.iter().find(|d| d.denom == denom.to_string());
    coin.map(|c| CheckedCoin::new(denom.clone(), AmountU128::new(c.amount)))
        .ok_or_else(|| StdError::not_found(denom.to_string()))
}

pub fn delegate_label(env: &Env) -> String {
    format!(
        "Unstake.fi delegate {}/{}",
        env.block.height,
        env.transaction
            .as_ref()
            .map(|x| x.index)
            .unwrap_or_default()
    )
}
