use std::cmp::Ordering;

use crate::config::Config;
use crate::state::State;
#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    ensure, ensure_eq, to_json_binary, wasm_execute, Addr, Binary, CosmosMsg, Decimal, Deps,
    DepsMut, Empty, Env, MessageInfo, Order, Response, StdError, StdResult, Uint128,
};
use cw2::set_contract_version;
use cw_storage_plus::Map;
use cw_utils::must_pay;
use kujira::{DenomMsg, KujiraMsg, KujiraQuery};
use kujira_ghost::basic_vault::DepositMsg;
use kujira_ghost::receipt_vault::{
    ExecuteMsg as GhostExecuteMsg, QueryMsg as GhostQueryMsg,
    StatusResponse as GhostStatusResponse, WithdrawMsg,
};
use unstake::math::{amt_to_rsv_tokens, rsv_tokens_to_amt};
use unstake::reserve::{
    ConfigResponse, ExecuteMsg, InstantiateMsg, QueryMsg, StatusResponse, WhitelistItem,
    WhitelistResponse,
};
use unstake::ContractError;

// version info for migration info
const CONTRACT_NAME: &str = "unstake/reserve";
const CONTRACT_VERSION: &str = env!("CARGO_PKG_VERSION");

pub const URSV: &str = "ursv";
pub const WHITELISTED_CONTROLLERS: Map<&Addr, (Uint128, Option<Uint128>)> =
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
            let base_amount = must_pay(&info, config.base_denom.as_ref())?;
            let ghost_deposit_msg = wasm_execute(
                &config.ghost_vault_addr,
                &GhostExecuteMsg::Deposit(DepositMsg { callback: None }),
                config.base_denom.coins(&base_amount),
            )?;

            // Calculate how much GHOST receipt we'll get
            let ghost_rates: GhostStatusResponse = deps
                .querier
                .query_wasm_smart(&config.ghost_vault_addr, &GhostQueryMsg::Status {})?;
            let ghost_amount = kujira_ghost::math::amt_to_rcpt_tokens(
                base_amount,
                ghost_rates.deposit_redemption_ratio,
            );

            state.total_deposits = state.total_deposits.checked_add(ghost_amount)?;
            state.save(deps.storage)?;

            // Mint appropriate amount of reserve tokens
            let reserve_mint_amount =
                amt_to_rsv_tokens(ghost_amount, state.reserve_redemption_ratio);
            let reserve_mint_msg = DenomMsg::Mint {
                denom: config.rsv_denom.clone(),
                amount: reserve_mint_amount,
                recipient: env.contract.address,
            };

            // Return or callback to sender
            let return_msg = match callback {
                Some(cb) => cb.to_message(
                    &info.sender,
                    &Empty {},
                    config.rsv_denom.coins(&reserve_mint_amount),
                )?,
                None => config.rsv_denom.send(&info.sender, &reserve_mint_amount),
            };

            Ok(Response::default()
                .add_message(ghost_deposit_msg)
                .add_message(reserve_mint_msg)
                .add_message(return_msg))
        }
        ExecuteMsg::Withdraw { callback } => {
            let reserve_amount = must_pay(&info, config.rsv_denom.as_ref())?;

            // Ensure we have enough liquidity to withdraw
            let ghost_amount = rsv_tokens_to_amt(reserve_amount, state.reserve_redemption_ratio);
            let liquidity = config
                .ghost_denom
                .query_balance(deps.querier, &env.contract.address)?
                .amount;
            if ghost_amount.gt(&liquidity) {
                return Err(ContractError::InsufficentFunds {});
            }

            // Burn reserve tokens
            let burn_msg = DenomMsg::Burn {
                denom: config.rsv_denom.clone(),
                amount: reserve_amount,
            };

            // Withdraw from GHOST vault
            let ghost_withdraw_msg = wasm_execute(
                &config.ghost_vault_addr,
                &GhostExecuteMsg::Withdraw(WithdrawMsg { callback: None }),
                config.ghost_denom.coins(&ghost_amount),
            )?;

            // Return or callback with base tokens to sender
            let ghost_rates: GhostStatusResponse = deps
                .querier
                .query_wasm_smart(&config.ghost_vault_addr, &GhostQueryMsg::Status {})?;
            let base_amount = kujira_ghost::math::rcpt_tokens_to_owed(
                ghost_amount,
                ghost_rates.deposit_redemption_ratio,
            );
            let return_msg = match callback {
                Some(cb) => cb.to_message(
                    &info.sender,
                    &Empty {},
                    config.base_denom.coins(&base_amount),
                )?,
                None => config.base_denom.send(&info.sender, &base_amount),
            };

            state.total_deposits = state.total_deposits.checked_sub(ghost_amount)?;
            state.save(deps.storage)?;

            Ok(Response::default()
                .add_message(burn_msg)
                .add_message(ghost_withdraw_msg)
                .add_message(return_msg))
        }
        ExecuteMsg::RequestReserves {
            requested_amount,
            callback,
        } => {
            let maybe_limit = WHITELISTED_CONTROLLERS.may_load(deps.storage, &info.sender)?;
            ensure!(maybe_limit.is_some(), ContractError::Unauthorized {});

            // Ensure we don't exceed the limit for this controller
            let (mut lent, limit) = maybe_limit.unwrap();
            lent = lent.checked_add(requested_amount)?;
            ensure!(
                limit.is_none() || lent.le(&limit.unwrap()),
                ContractError::ControllerLimitExceeded {}
            );
            WHITELISTED_CONTROLLERS.save(deps.storage, &info.sender, &(lent, limit))?;

            // Ensure we have enough liquidity to allocate the requested amount
            let liquidity = config
                .ghost_denom
                .query_balance(deps.querier, &env.contract.address)?
                .amount;
            if requested_amount.gt(&liquidity) {
                return Err(ContractError::InsufficentReserves {});
            }

            // Send or callback with requested amount
            let return_msg = match callback {
                Some(cb) => cb.to_message(
                    &info.sender,
                    &Empty {},
                    config.ghost_denom.coins(&requested_amount),
                )?,
                None => config.ghost_denom.send(&info.sender, &requested_amount),
            };

            Ok(Response::default().add_message(return_msg))
        }
        ExecuteMsg::ReturnReserves {
            original_amount,
            callback,
        } => {
            let maybe_limit = WHITELISTED_CONTROLLERS.may_load(deps.storage, &info.sender)?;
            ensure!(maybe_limit.is_some(), ContractError::Unauthorized {});

            // Ensure we only receive ghost tokens
            let ghost_amount = must_pay(&info, config.ghost_denom.as_ref())?;

            // Update the controller's lent amount
            let (mut lent, limit) = maybe_limit.unwrap();
            lent = lent.checked_sub(original_amount)?;
            WHITELISTED_CONTROLLERS.save(deps.storage, &info.sender, &(lent, limit))?;

            // Update the rates
            match ghost_amount.cmp(&original_amount) {
                Ordering::Less => {
                    // Loss, so decrease rates
                    let loss_amount = original_amount.checked_sub(ghost_amount)?;
                    let ratio_delta = Decimal::from_ratio(loss_amount, state.total_deposits);
                    state.reserve_redemption_ratio =
                        state.reserve_redemption_ratio.checked_sub(ratio_delta)?;
                }
                Ordering::Equal => {
                    // No change, but this is very unlikely
                }
                Ordering::Greater => {
                    // Profit, so increase rates
                    let profit_amount = ghost_amount.checked_sub(original_amount)?;
                    let ratio_delta = Decimal::from_ratio(profit_amount, state.total_deposits);
                    state.reserve_redemption_ratio =
                        state.reserve_redemption_ratio.checked_add(ratio_delta)?;
                }
            }
            state.save(deps.storage)?;

            // If callback, send the callback message.
            let return_msg = match callback {
                Some(cb) => vec![cb.to_message(&info.sender, &Empty {}, vec![])?],
                None => vec![],
            };

            Ok(Response::default().add_messages(return_msg))
        }
        ExecuteMsg::AddController { controller, limit } => {
            ensure_eq!(info.sender, config.owner, ContractError::Unauthorized {});
            WHITELISTED_CONTROLLERS.save(deps.storage, &controller, &(Uint128::zero(), limit))?;
            Ok(Response::default())
        }
        ExecuteMsg::RemoveController { controller } => {
            ensure_eq!(info.sender, config.owner, ContractError::Unauthorized {});
            WHITELISTED_CONTROLLERS.remove(deps.storage, &controller);
            Ok(Response::default())
        }
        ExecuteMsg::UpdateConfig { owner } => {
            ensure_eq!(info.sender, config.owner, ContractError::Unauthorized {});
            let mut config = Config::load(deps.storage)?;
            config.update(owner);
            config.save(deps.storage)?;
            Ok(Response::default())
        }
        ExecuteMsg::MigrateLegacyReserve { reserves_deployed } => todo!(),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<KujiraQuery>, env: Env, msg: QueryMsg) -> Result<Binary, ContractError> {
    let config = Config::load(deps.storage)?;
    match msg {
        QueryMsg::Status {} => {
            let state = State::load(deps.storage)?;
            let ghost_not_lent = config
                .ghost_denom
                .query_balance(deps.querier, &env.contract.address)?
                .amount;
            let total_ghost_amount =
                rsv_tokens_to_amt(state.total_deposits, state.reserve_redemption_ratio);
            let total_ghost_lent = total_ghost_amount.checked_sub(ghost_not_lent)?;

            Ok(to_json_binary(&StatusResponse {
                total_deposited: state.total_deposits,
                reserves_deployed: total_ghost_lent,
                reserves_available: ghost_not_lent,
                reserve_redemption_rate: state.reserve_redemption_ratio,
            })?)
        }
        QueryMsg::Whitelist {} => {
            let whitelist = WHITELISTED_CONTROLLERS
                .range(deps.storage, None, None, Order::Ascending)
                .map(|item| {
                    let (addr, (lent, limit)) = item?;
                    Ok(WhitelistItem {
                        controller: addr,
                        lent,
                        limit,
                    })
                })
                .collect::<Result<_, StdError>>()?;
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
