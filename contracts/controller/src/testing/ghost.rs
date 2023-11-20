use std::str::FromStr;

#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, CosmosMsg, Decimal, Deps, DepsMut, Empty, Env, MessageInfo, Response,
    StdResult, Uint128,
};
use cw_storage_plus::Item;
use cw_utils::NativeBalance;
use kujira::{Denom, DenomMsg, KujiraMsg, KujiraQuery};
use kujira_ghost::receipt_vault::{ExecuteMsg, InstantiateMsg, QueryMsg, StatusResponse};

static INIT: Item<InstantiateMsg> = Item::new("init");

#[cfg_attr(not(feature = "library"), entry_point)]
pub fn instantiate(
    deps: DepsMut<KujiraQuery>,
    _env: Env,
    _info: MessageInfo,
    msg: InstantiateMsg,
) -> StdResult<Response<KujiraMsg>> {
    INIT.save(deps.storage, &msg)?;
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
        ExecuteMsg::Repay(_) => todo!(),
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
