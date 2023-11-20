use std::{ops::Add, str::FromStr};

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, Coin, CosmosMsg, Decimal, Deps, DepsMut, Empty, Env, MessageInfo,
    Response, StdResult, Timestamp, Uint128,
};
use cw_storage_plus::Item;
use cw_utils::NativeBalance;
use kujira::{Denom, DenomMsg, KujiraMsg, KujiraQuery};
use kujira_ghost::receipt_vault::{ExecuteMsg, InstantiateMsg, QueryMsg, StatusResponse};

static INIT: Item<InstantiateMsg> = Item::new("init");
static TS: Item<Timestamp> = Item::new("ts");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<KujiraQuery>,
    env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response<KujiraMsg>> {
    INIT.save(deps.storage, &msg)?;
    TS.save(deps.storage, &env.block.time)?;
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
            let debt_share_ratio = Decimal::from_str("1.12")?;
            let debt_shares = msg.amount.div_ceil(debt_share_ratio);
            TS.save(deps.storage, &env.block.time)?;

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

            let interest_rate = Decimal::one();
            let ts = TS.load(deps.storage)?;
            let delta = Decimal::from_ratio(env.block.time.seconds(), ts.seconds()) * interest_rate;
            let original_debt_rate = Decimal::from_str("1.12")?;

            let debt_rate = original_debt_rate * Decimal::one().add(delta);
            let repay_requirement = debt_tokens.mul_ceil(debt_rate);
            if repay_requirement.ne(&repay_amount) {
                return Err(cosmwasm_std::StdError::GenericErr {
                    msg: "Insufficient repay amount".to_string(),
                });
            }
            // Basic assertion that the repay amount

            Ok(Response::default())
        }
        ExecuteMsg::WhitelistMarket(_) => todo!(),
        ExecuteMsg::UpdateMarket(_) => todo!(),
        ExecuteMsg::UpdateConfig(_) => todo!(),
        ExecuteMsg::UpdateInterest(_) => todo!(),
    }
}

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn query(_deps: Deps<KujiraQuery>, _env: Env, msg: QueryMsg) -> StdResult<Binary> {
    match msg {
        QueryMsg::Config {} => todo!(),
        QueryMsg::Status {} => to_json_binary(&StatusResponse {
            deposited: Uint128::zero(),
            borrowed: Uint128::zero(),
            rate: Decimal::one(),
            deposit_redemption_ratio: Decimal::one(),
            debt_share_ratio: Decimal::one(),
        }),
        QueryMsg::MarketParams { .. } => todo!(),
        QueryMsg::Markets { .. } => todo!(),
    }
}
