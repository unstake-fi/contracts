use cosmwasm_std::{coins, Addr, Uint128};
use cw_multi_test::{AppResponse, Executor};
use kujira_rs_testing::mock::CustomApp;
use monetary::AmountU128;
use unstake::reserve::{ConfigResponse, ExecuteMsg, QueryMsg, StatusResponse, WhitelistResponse};

use super::tests::Contracts;

pub fn fund(
    app: &mut CustomApp,
    contracts: &Contracts,
    sender: &Addr,
    amount: Uint128,
) -> anyhow::Result<AppResponse> {
    app.execute_contract(
        sender.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::Fund { callback: None },
        &coins(amount.u128(), "base"),
    )
}

pub fn withdraw(
    app: &mut CustomApp,
    contracts: &Contracts,
    sender: &Addr,
    amount: Uint128,
) -> anyhow::Result<AppResponse> {
    let ursv = format!("factory/{}/ursv", contracts.reserve);
    app.execute_contract(
        sender.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::Withdraw { callback: None },
        &coins(amount.u128(), ursv),
    )
}

pub fn request_reserves(
    app: &mut CustomApp,
    contracts: &Contracts,
    controller: &Addr,
    amount: Uint128,
) -> anyhow::Result<AppResponse> {
    app.execute_contract(
        controller.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::RequestReserves {
            requested_amount: AmountU128::new(amount),
            callback: None,
        },
        &[],
    )
}

pub fn return_reserves(
    app: &mut CustomApp,
    contracts: &Contracts,
    controller: &Addr,
    original_amount: Uint128,
    returned_amount: Uint128,
) -> anyhow::Result<AppResponse> {
    app.execute_contract(
        controller.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::ReturnReserves {
            original_amount: AmountU128::new(original_amount),
            callback: None,
        },
        &coins(returned_amount.u128(), "base"),
    )
}

pub fn add_controller(
    app: &mut CustomApp,
    contracts: &Contracts,
    owner: &Addr,
    controller: &Addr,
    limit: Uint128,
) -> anyhow::Result<AppResponse> {
    app.execute_contract(
        owner.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::AddController {
            controller: controller.clone(),
            limit: Some(AmountU128::new(limit)),
        },
        &[],
    )
}

pub fn remove_controller(
    app: &mut CustomApp,
    contracts: &Contracts,
    owner: &Addr,
    controller: &Addr,
) -> anyhow::Result<AppResponse> {
    app.execute_contract(
        owner.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::RemoveController {
            controller: controller.clone(),
        },
        &[],
    )
}

pub fn update_config(
    app: &mut CustomApp,
    contracts: &Contracts,
    owner: &Addr,
    new_owner: Addr,
) -> anyhow::Result<AppResponse> {
    app.execute_contract(
        owner.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::UpdateConfig {
            owner: Some(new_owner),
        },
        &[],
    )
}

pub fn query_status(app: &CustomApp, contracts: &Contracts) -> StatusResponse {
    app.wrap()
        .query_wasm_smart(contracts.reserve.clone(), &QueryMsg::Status {})
        .unwrap()
}

pub fn query_whitelist(app: &CustomApp, contracts: &Contracts) -> WhitelistResponse {
    app.wrap()
        .query_wasm_smart(contracts.reserve.clone(), &QueryMsg::Whitelist {})
        .unwrap()
}

pub fn query_config(app: &CustomApp, contracts: &Contracts) -> ConfigResponse {
    app.wrap()
        .query_wasm_smart(contracts.reserve.clone(), &QueryMsg::Config {})
        .unwrap()
}
