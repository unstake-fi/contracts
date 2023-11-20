#[cfg(not(feature = "library"))]
use cosmwasm_std::entry_point;
use cosmwasm_std::{
    to_json_binary, Binary, Decimal, Deps, DepsMut, Env, MessageInfo, Response, StdResult, Uint128,
};
use cw_storage_plus::Item;
use kujira::{KujiraMsg, KujiraQuery};
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
    _deps: DepsMut<KujiraQuery>,
    _env: Env,
    _info: MessageInfo,
    _msg: ExecuteMsg,
) -> StdResult<Response<KujiraMsg>> {
    todo!()
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
