use std::cmp::Ordering;

use crate::config::Config;
use crate::state::{State, LEGACY_DENOMS};
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    coins, ensure, ensure_eq, to_json_binary, wasm_execute, Addr, Binary, CosmosMsg, CustomQuery,
    Decimal, Deps, DepsMut, Empty, Env, Event, MessageInfo, Order, QuerierWrapper, Response,
    StdResult,
};
use cw2::set_contract_version;
use cw_storage_plus::Map;
use cw_utils::{one_coin, PaymentError};
use kujira::{DenomMsg, KujiraMsg, KujiraQuery};
use kujira_ghost::basic_vault::DepositMsg;
use kujira_ghost::receipt_vault::{
    ExecuteMsg as GhostExecuteMsg, QueryMsg as GhostQueryMsg,
    StatusResponse as GhostStatusResponse, WithdrawMsg,
};
use monetary::{must_pay, AmountU128, Exchange, Rate};
use unstake::denoms::{Base, LegacyRsv, Rcpt};
use unstake::reserve::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, StatusResponse, WhitelistItem,
    WhitelistResponse,
};
use unstake::ContractError;

// version info for migration info
const CONTRACT_NAME: &str = "unstake/reserve";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const URSV: &str = "ursv";
pub const WHITELISTED_CONTROLLERS: Map<&Addr, (AmountU128<Base>, Option<AmountU128<Base>>)> =
    Map::new("whitelisted_controllers");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> Result<Response<KujiraMsg>, ContractError> {
    set_contract_version(deps.storage, CONTRACT_NAME, CONTRACT_VERSION)?;
    let config = Config::new(msg, &deps.querier, &env)?;
    config.save(deps.storage)?;

    let state = State::default();
    state.save(deps.storage)?;

    let create_msg: CosmosMsg<KujiraMsg> = DenomMsg::Create {
        subdenom: URSV.into(),
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
    let mut state = State::load(deps.storage)?;
    match msg {
        ExecuteMsg::Fund { callback } => {
            // Deposit to GHOST vault
            let base_amount = must_pay(&info, &config.base_denom)?;
            let ghost_deposit_msg = wasm_execute(
                &config.ghost_vault_addr,
                &GhostExecuteMsg::Deposit(DepositMsg { callback: None }),
                coins(base_amount.u128(), &config.base_denom),
            )?;

            let ghost_rate = ghost_rate(&deps.querier, &config)?;
            let received = base_amount.div_floor(&ghost_rate);

            state.available += received;
            state.save(deps.storage)?;

            // Mint appropriate amount of reserve tokens
            let reserve_mint_amount = base_amount.div_floor(&state.reserve_redemption_ratio);
            let reserve_mint_msg = DenomMsg::Mint {
                denom: config.rsv_denom.to_string().into(),
                amount: reserve_mint_amount.uint128(),
                recipient: env.contract.address,
            };

            // Return or callback to sender
            let return_msg = match callback {
                Some(cb) => cb.to_message(
                    &info.sender,
                    &Empty {},
                    vec![config.rsv_denom.coin(reserve_mint_amount).into()],
                )?,
                None => config
                    .rsv_denom
                    .send(&info.sender, reserve_mint_amount)
                    .into(),
            };

            let event = Event::new("unstake/reserve/fund").add_attributes(vec![
                ("fund_amount", &base_amount.to_string()),
                ("rsv_amount", &reserve_mint_amount.to_string()),
                ("total_available", &state.available.to_string()),
                ("sender", &info.sender.to_string()),
            ]);

            Ok(Response::default()
                .add_message(ghost_deposit_msg)
                .add_message(reserve_mint_msg)
                .add_message(return_msg)
                .add_event(event))
        }
        ExecuteMsg::Withdraw { callback } => {
            let reserve_amount = must_pay(&info, &config.rsv_denom)?;

            // Ensure we have enough liquidity to withdraw
            let ghost_rate = ghost_rate(&deps.querier, &config)?;
            let rate = ghost_rate.inv() * state.reserve_redemption_ratio;

            let liquidity = state.available;
            let required_liquidity = reserve_amount.mul_ceil(&rate);
            if liquidity.lt(&required_liquidity) {
                return Err(ContractError::InsufficentFunds {});
            }

            state.available -= required_liquidity;
            state.save(deps.storage)?;

            // Burn reserve tokens
            let burn_msg = DenomMsg::Burn {
                denom: config.rsv_denom.to_string().into(),
                amount: reserve_amount.uint128(),
            };

            // Withdraw from GHOST vault
            let ghost_withdraw_msg = wasm_execute(
                &config.ghost_vault_addr,
                &GhostExecuteMsg::Withdraw(WithdrawMsg { callback: None }),
                vec![config.ghost_denom.coin(required_liquidity).into()],
            )?;

            // Return or callback with base tokens to sender
            let base_amount = required_liquidity.mul_floor(&ghost_rate);
            let return_msg = match callback {
                Some(cb) => cb.to_message(
                    &info.sender,
                    &Empty {},
                    vec![config.base_denom.coin(base_amount).into()],
                )?,
                None => config.base_denom.send(&info.sender, base_amount).into(),
            };

            let event = Event::new("unstake/reserve/withdraw").add_attributes(vec![
                ("rsv_amount", &reserve_amount.to_string()),
                ("base_amount", &base_amount.to_string()),
                ("total_available", &state.available.to_string()),
                ("sender", &info.sender.to_string()),
            ]);

            Ok(Response::default()
                .add_message(burn_msg)
                .add_message(ghost_withdraw_msg)
                .add_message(return_msg)
                .add_event(event))
        }
        ExecuteMsg::RequestReserves {
            requested_amount,
            callback,
        } => {
            let maybe_limit = WHITELISTED_CONTROLLERS.may_load(deps.storage, &info.sender)?;
            ensure!(maybe_limit.is_some(), ContractError::Unauthorized {});
            ensure!(!requested_amount.is_zero(), ContractError::RequestZero {});

            // Ensure we don't exceed the limit for this controller
            let (mut lent, limit) = maybe_limit.unwrap();
            lent = lent.checked_add(requested_amount)?;
            ensure!(
                limit.is_none() || lent.le(&limit.unwrap()),
                ContractError::ControllerLimitExceeded {}
            );
            WHITELISTED_CONTROLLERS.save(deps.storage, &info.sender, &(lent, limit))?;

            // Ensure we have enough liquidity to allocate the requested amount
            let ghost_rate = ghost_rate(&deps.querier, &config)?;
            let required_liquidity = requested_amount.div_ceil(&ghost_rate);
            if required_liquidity.gt(&state.available) {
                return Err(ContractError::InsufficentReserves {});
            }

            state.available -= required_liquidity;
            state.deployed += requested_amount;
            state.save(deps.storage)?;

            // Withdraw required amount from GHOST vault
            let ghost_withdraw_msg = wasm_execute(
                &config.ghost_vault_addr,
                &GhostExecuteMsg::Withdraw(WithdrawMsg { callback: None }),
                vec![config.ghost_denom.coin(required_liquidity).into()],
            )?;

            // Send or callback with requested amount
            let return_msg = match callback {
                Some(cb) => cb.to_message(
                    &info.sender,
                    &Empty {},
                    vec![config.base_denom.coin(requested_amount).into()],
                )?,
                None => config
                    .base_denom
                    .send(&info.sender, requested_amount)
                    .into(),
            };

            let event = Event::new("unstake/reserve/request").add_attributes(vec![
                ("requested_amount", &requested_amount.to_string()),
                ("total_available", &state.available.to_string()),
                ("total_deployed", &state.deployed.to_string()),
                ("controller", &info.sender.to_string()),
                ("controller_lent", &lent.to_string()),
            ]);

            Ok(Response::default()
                .add_message(ghost_withdraw_msg)
                .add_message(return_msg)
                .add_event(event))
        }
        ExecuteMsg::ReturnReserves {
            original_amount,
            callback,
        } => {
            let maybe_limit = WHITELISTED_CONTROLLERS.may_load(deps.storage, &info.sender)?;
            ensure!(maybe_limit.is_some(), ContractError::Unauthorized {});

            let received = must_pay(&info, &config.base_denom)?;

            // Update the controller's lent amount
            let (mut lent, limit) = maybe_limit.unwrap();
            lent = lent.checked_sub(original_amount)?;
            WHITELISTED_CONTROLLERS.save(deps.storage, &info.sender, &(lent, limit))?;

            // Update the rates
            let ghost_rate = ghost_rate(&deps.querier, &config)?;
            let total_base = state.deployed + state.available.mul_floor(&ghost_rate);
            let delta = Decimal::from_ratio(
                original_amount.abs_diff(received).uint128(),
                total_base.uint128(),
            );
            state.reserve_redemption_ratio = match received.cmp(&original_amount) {
                Ordering::Less => state.reserve_redemption_ratio.sub_decimal(delta)?,
                Ordering::Equal => state.reserve_redemption_ratio,
                Ordering::Greater => state.reserve_redemption_ratio.add_decimal(delta)?,
            };

            // Deposit the returned amount to the GHOST vault
            let ghost_deposit_msg = wasm_execute(
                &config.ghost_vault_addr,
                &GhostExecuteMsg::Deposit(DepositMsg { callback: None }),
                vec![config.base_denom.coin(received).into()],
            )?;
            state.available += received.div_floor(&ghost_rate);
            state.deployed -= original_amount;

            state.save(deps.storage)?;

            // If callback, send the callback message.
            let return_msg = match callback {
                Some(cb) => vec![cb.to_message(&info.sender, &Empty {}, vec![])?],
                None => vec![],
            };

            let event = Event::new("unstake/reserve/return").add_attributes(vec![
                ("original_amount", &original_amount.to_string()),
                ("received_amount", &received.to_string()),
                ("total_available", &state.available.to_string()),
                ("total_deployed", &state.deployed.to_string()),
                ("controller", &info.sender.to_string()),
                ("controller_lent", &lent.to_string()),
            ]);

            Ok(Response::default()
                .add_message(ghost_deposit_msg)
                .add_messages(return_msg)
                .add_event(event))
        }
        ExecuteMsg::AddController { controller, limit } => {
            ensure_eq!(info.sender, config.owner, ContractError::Unauthorized {});
            WHITELISTED_CONTROLLERS.update(deps.storage, &controller, |c| {
                StdResult::Ok(c.map_or((AmountU128::zero(), limit), |(lent, _)| (lent, limit)))
            })?;

            let event = Event::new("unstake/reserve/add_controller").add_attributes(vec![
                ("controller", controller.to_string()),
                ("limit", limit.map_or("null".to_string(), |l| l.to_string())),
            ]);
            Ok(Response::default().add_event(event))
        }
        ExecuteMsg::RemoveController { controller } => {
            ensure_eq!(info.sender, config.owner, ContractError::Unauthorized {});
            WHITELISTED_CONTROLLERS.remove(deps.storage, &controller);

            let event = Event::new("unstake/reserve/remove_controller")
                .add_attributes(vec![("controller", controller.to_string())]);
            Ok(Response::default().add_event(event))
        }
        ExecuteMsg::UpdateConfig { owner } => {
            ensure_eq!(info.sender, config.owner, ContractError::Unauthorized {});
            let mut config = Config::load(deps.storage)?;
            config.update(owner);
            config.save(deps.storage)?;
            Ok(Response::default())
        }
        ExecuteMsg::MigrateLegacyReserve {
            reserves_deployed,
            legacy_denom,
            legacy_redemption_rate,
        } => {
            // Assert authorized controller
            let maybe_limit = WHITELISTED_CONTROLLERS.may_load(deps.storage, &info.sender)?;
            ensure!(maybe_limit.is_some(), ContractError::Unauthorized {});

            // Add the "deployed" amount to the controller's lent amount
            let (mut lent, limit) = maybe_limit.unwrap();
            lent = lent.checked_add(reserves_deployed)?;
            WHITELISTED_CONTROLLERS.save(deps.storage, &info.sender, &(lent, limit))?;

            // Deposit to GHOST
            let base_amount = must_pay(&info, &config.base_denom)?;
            let ghost_rate = ghost_rate(&deps.querier, &config)?;
            let received = base_amount.div_floor(&ghost_rate);
            let ghost_msg = wasm_execute(
                &config.ghost_vault_addr,
                &GhostExecuteMsg::Deposit(DepositMsg { callback: None }),
                coins(base_amount.u128(), &config.base_denom),
            )?;

            // Update state. Available reserves are increased by the received amount from GHOST,
            // and deployed amount is specified by the controller that we're migrating from.
            state.available += received;
            state.deployed += reserves_deployed;
            state.save(deps.storage)?;

            // Snapshots the current rate of Legacy Reserve to Reserve, and saves it.
            let legacy_to_rsv = state.reserve_redemption_ratio.inv() * legacy_redemption_rate;
            LEGACY_DENOMS.save(deps.storage, legacy_denom.to_string(), &legacy_to_rsv)?;

            let event = Event::new("unstake/reserve/migrate").add_attributes(vec![
                ("base_amount", &base_amount.to_string()),
                ("received_amount", &received.to_string()),
                ("reserves_deployed", &reserves_deployed.to_string()),
                ("legacy_denom", &legacy_denom.to_string()),
                (
                    "legacy_redemption_rate",
                    &legacy_redemption_rate.to_string(),
                ),
                ("total_available", &state.available.to_string()),
                ("total_deployed", &state.deployed.to_string()),
                ("controller", &info.sender.to_string()),
                ("controller_lent", &lent.to_string()),
            ]);

            Ok(Response::default().add_message(ghost_msg).add_event(event))
        }
        ExecuteMsg::ExchangeLegacyReserve {} => {
            let received = one_coin(&info)?;
            let rate = LEGACY_DENOMS
                .may_load(deps.storage, received.denom.clone())?
                .ok_or(PaymentError::ExtraDenom(received.denom.clone()))?;

            let amount = AmountU128::<LegacyRsv>::new(received.amount);
            let return_amount = amount.mul_floor(&rate);

            let mint_msg = DenomMsg::Mint {
                denom: config.rsv_denom.to_string().into(),
                amount: return_amount.uint128(),
                recipient: info.sender.clone(),
            };

            let burn_msg = DenomMsg::Burn {
                denom: received.denom.clone().into(),
                amount: received.amount,
            };

            let event = Event::new("unstake/reserve/exchange_legacy").add_attributes(vec![
                ("legacy_denom", &received.denom),
                ("legacy_amount", &received.amount.to_string()),
                ("rsv_amount", &return_amount.to_string()),
                ("sender", &info.sender.to_string()),
            ]);

            Ok(Response::default()
                .add_message(mint_msg)
                .add_message(burn_msg)
                .add_event(event))
        }
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<KujiraQuery>, _env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    let config = Config::load(deps.storage)?;
    match msg {
        QueryMsg::Status {} => {
            let state = State::load(deps.storage)?;
            let ghost_rate = ghost_rate(&deps.querier, &config)?;
            let total_base = state.deployed + state.available.mul_floor(&ghost_rate);

            Ok(to_json_binary(&StatusResponse {
                total: total_base,
                deployed: state.deployed,
                available: state.available,
                reserve_redemption_rate: state.reserve_redemption_ratio,
            })?)
        }
        QueryMsg::Whitelist {} => {
            let whitelist = WHITELISTED_CONTROLLERS
                .range(deps.storage, None, None, Order::Ascending)
                .map(|item| {
                    let (controller, (lent, limit)) = item?;
                    Ok(WhitelistItem {
                        controller,
                        lent,
                        limit,
                    })
                })
                .collect::<StdResult<_>>()?;
            Ok(to_json_binary(&WhitelistResponse {
                controllers: whitelist,
            })?)
        }
        QueryMsg::Config {} => Ok(to_json_binary(&ConfigResponse::from(config))?),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn migrate(_deps: DepsMut<KujiraQuery>, _env: Env, _msg: ()) -> StdResult<Response<KujiraMsg>> {
    Ok(Response::default())
}

pub fn ghost_rate<C: CustomQuery>(
    querier: &QuerierWrapper<C>,
    config: &Config,
) -> StdResult<Rate<Base, Rcpt>> {
    let ghost_rates: GhostStatusResponse =
        querier.query_wasm_smart(&config.ghost_vault_addr, &GhostQueryMsg::Status {})?;
    Ok(Rate::new(ghost_rates.deposit_redemption_ratio).unwrap())
}
