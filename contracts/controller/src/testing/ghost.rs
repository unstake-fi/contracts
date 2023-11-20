use std::{
    ops::{Add, Mul},
    str::FromStr,
};

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, Coin, CosmosMsg, CustomQuery, Decimal, Deps, DepsMut, Empty, Env,
    MessageInfo, Response, StdResult, Timestamp, Uint128,
};
use cw_storage_plus::Item;
use cw_utils::NativeBalance;
use kujira::{Denom, DenomMsg, KujiraMsg, KujiraQuery};
use kujira_ghost::receipt_vault::{ExecuteMsg, InstantiateMsg, QueryMsg, StatusResponse};

static INIT: Item<InstantiateMsg> = Item::new("init");
static TS: Item<(Timestamp, Decimal)> = Item::new("ts");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response<KujiraMsg>> {
    INIT.save(deps.storage, &msg)?;
    TS.save(deps.storage, &(env.block.time, Decimal::from_str("1.12")?))?;
    Ok(Response::default())
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn execute(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    info: MessageInfo,
    msg: ExecuteMsg,
) -> StdResult<Response<KujiraMsg>> {
    let init = INIT.load(deps.storage)?;
    let debt_token_denom = Denom::from(format!(
        "factory/{}/{}",
        env.contract.address, init.debt_token_denom
    ));
    let denom = init.denom;
    match msg {
        ExecuteMsg::Deposit(_) => Ok(Response::default()),
        ExecuteMsg::Withdraw(_) => todo!(),
        ExecuteMsg::Borrow(msg) => {
            let (_rate, debt_share_ratio) = rates(deps.as_ref(), env.block.time)?;
            let debt_shares = msg.amount.div_ceil(debt_share_ratio);
            TS.save(deps.storage, &(env.block.time, debt_share_ratio))?;

            let debt_mint_msg = CosmosMsg::Custom(KujiraMsg::Denom(DenomMsg::Mint {
                denom: debt_token_denom.clone().into(),
                amount: debt_shares,
                recipient: env.contract.address,
            }));

            let mut to_send = NativeBalance(vec![
                denom.coin(&msg.amount),
                debt_token_denom.coin(&debt_shares),
            ]);
            to_send.normalize();

            let borrow_msg = msg.callback.map_or_else(
                || {
                    CosmosMsg::Bank(cosmwasm_std::BankMsg::Send {
                        to_address: info.sender.to_string(),
                        amount: to_send.clone().into_vec(),
                    })
                },
                |cb| {
                    cb.to_message(&info.sender, Empty {}, to_send.clone().into_vec())
                        .unwrap()
                },
            );
            Ok(Response::default().add_messages(vec![debt_mint_msg, borrow_msg]))
        }
        ExecuteMsg::Repay(_) => {
            let mut debt_tokens = Uint128::zero();
            let mut repay_amount = Uint128::zero();

            for Coin { amount, denom } in info.funds {
                if denom == debt_token_denom.to_string() {
                    debt_tokens = amount
                }

                if denom == denom.to_string() {
                    repay_amount = amount
                }
            }
            let (_rate, debt_share_ratio) = rates(deps.as_ref(), env.block.time)?;

            let repay_requirement = debt_tokens.mul_ceil(debt_share_ratio);

            let debt_burn_msg = CosmosMsg::Custom(KujiraMsg::Denom(DenomMsg::Burn {
                denom: debt_token_denom.clone().into(),
                amount: debt_tokens,
            }));

            if repay_requirement.ne(&repay_amount) {
                return Err(cosmwasm_std::StdError::GenericErr {
                    msg: "Insufficient repay amount".to_string(),
                });
            }
            // Basic assertion that the repay amount

            Ok(Response::default().add_message(debt_burn_msg))
        }
        ExecuteMsg::WhitelistMarket(_) => todo!(),
        ExecuteMsg::UpdateMarket(_) => todo!(),
        ExecuteMsg::UpdateConfig(_) => todo!(),
        ExecuteMsg::UpdateInterest(_) => todo!(),
    }
}

fn rates<T: CustomQuery>(deps: Deps<T>, now: Timestamp) -> StdResult<(Decimal, Decimal)> {
    let interest_rate = Decimal::one();
    let (last_ts, last_rate) = TS.load(deps.storage)?;
    let delta = Decimal::from_ratio(now.seconds() - last_ts.seconds(), 365u128 * 24 * 60 * 60);
    let debt_rate = last_rate.mul(Decimal::one().add(delta * interest_rate));

    Ok((interest_rate, debt_rate))
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(deps: Deps<KujiraQuery>, env: Env, msg: QueryMsg) -> StdResult<Binary> {
    let (rate, debt_share_ratio) = rates(deps, env.block.time)?;
    match msg {
        QueryMsg::Config {} => todo!(),
        QueryMsg::Status {} => to_json_binary(&StatusResponse {
            deposited: Uint128::zero(),
            borrowed: Uint128::zero(),
            rate,
            deposit_redemption_ratio: Decimal::zero(),
            debt_share_ratio,
        }),
        QueryMsg::MarketParams { .. } => todo!(),
        QueryMsg::Markets { .. } => todo!(),
    }
}
