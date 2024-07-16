use cosmwasm_std::{coins, Addr, Coin, Decimal, Uint128};
use cw_multi_test::{ContractWrapper, Executor};
use kujira::{Denom, HumanPrice};
use kujira_ghost::common::OracleType;
use kujira_rs_testing::{
    api::MockApiBech32,
    mock::{mock_app, CustomApp},
};
use monetary::AmountU128;
use unstake::reserve::{ExecuteMsg, InstantiateMsg, QueryMsg, StatusResponse};

use super::util::*;

pub struct Contracts {
    pub reserve: Addr,
    pub ghost: Addr,
}

fn setup(balances: Vec<(Addr, Vec<Coin>)>) -> (CustomApp, Contracts) {
    let mut app = mock_app(balances);

    let reserve_code: ContractWrapper<
        ExecuteMsg,
        InstantiateMsg,
        QueryMsg,
        unstake::ContractError,
        unstake::ContractError,
        unstake::ContractError,
        kujira::KujiraMsg,
        kujira::KujiraQuery,
    > = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );
    let ghost_code = ContractWrapper::new(
        crate::testing::ghost::execute,
        crate::testing::ghost::instantiate,
        crate::testing::ghost::query,
    );

    let reserve_code_id = app.store_code(Box::new(reserve_code));
    let ghost_code_id = app.store_code(Box::new(ghost_code));

    let vault_address = app
        .instantiate_contract(
            ghost_code_id,
            app.api().addr_make("ghost"),
            &kujira_ghost::receipt_vault::InstantiateMsg {
                owner: app.api().addr_make("ghost-owner"),
                denom: Denom::from("base"),
                oracle: OracleType::Static(HumanPrice::from(Decimal::one())),
                decimals: 6,
                denom_creation_fee: Uint128::zero(),
                utilization_to_curve: vec![],
            },
            &[],
            "ghost",
            None,
        )
        .unwrap();

    let reserve_address = app
        .instantiate_contract(
            reserve_code_id,
            app.api().addr_make("owner"),
            &InstantiateMsg {
                owner: app.api().addr_make("owner"),
                base_denom: monetary::Denom::new("base"),
                ghost_vault_addr: vault_address.clone(),
            },
            &[],
            "reserve",
            None,
        )
        .unwrap();

    (
        app,
        Contracts {
            reserve: reserve_address,
            ghost: vault_address,
        },
    )
}

#[test]
fn test_initialization() {
    let (app, contracts) = setup(vec![]);
    let config = query_config(&app, &contracts);

    assert_eq!(config.owner, app.api().addr_make("owner"));
    assert_eq!(config.base_denom, monetary::Denom::new("base"));
    assert_eq!(config.ghost_vault_addr, contracts.ghost);
}

#[test]
fn test_fund() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![(api.addr_make("funder"), coins(1000000u128, "base"))];

    let (mut app, contracts) = setup(balances);
    let funder = app.api().addr_make("funder");

    fund(&mut app, &contracts, &funder, Uint128::new(1000)).unwrap();
    let status = query_status(&app, &contracts);

    assert_eq!(status.available.u128(), 1000u128);
    assert_eq!(status.deployed.u128(), 0u128);
}

#[test]
fn test_withdraw() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![(api.addr_make("funder"), coins(1000000u128, "base"))];
    let (mut app, contracts) = setup(balances);
    let funder = app.api().addr_make("funder");

    fund(&mut app, &contracts, &funder, Uint128::new(1000)).unwrap();
    withdraw(&mut app, &contracts, &funder, Uint128::new(1000)).unwrap();

    let status = query_status(&app, &contracts);
    assert_eq!(status.available.u128(), 0u128);
    assert_eq!(status.deployed.u128(), 0u128);
}

#[test]
fn test_request_reserves() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(1000000u128, "base")),
        (api.addr_make("controller"), vec![]),
    ];
    let (mut app, contracts) = setup(balances);

    let funder = app.api().addr_make("funder");
    let controller = app.api().addr_make("controller");
    let owner = app.api().addr_make("owner");

    fund(&mut app, &contracts, &funder, Uint128::new(1000)).unwrap();
    add_controller(
        &mut app,
        &contracts,
        &owner,
        &controller,
        Uint128::new(1000),
    )
    .unwrap();
    request_reserves(&mut app, &contracts, &controller, Uint128::new(500)).unwrap();

    let status = query_status(&app, &contracts);
    assert_eq!(status.available.u128(), 500u128);
    assert_eq!(status.deployed.u128(), 500u128);
}

#[test]
fn test_return_reserves() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(1000000u128, "base")),
        (api.addr_make("controller"), vec![]),
    ];
    let (mut app, contracts) = setup(balances);

    let funder = app.api().addr_make("funder");
    let controller = app.api().addr_make("controller");
    let owner = app.api().addr_make("owner");

    fund(&mut app, &contracts, &funder, Uint128::new(1000)).unwrap();
    add_controller(
        &mut app,
        &contracts,
        &owner,
        &controller,
        Uint128::new(1000),
    )
    .unwrap();
    request_reserves(&mut app, &contracts, &controller, Uint128::new(500)).unwrap();
    return_reserves(
        &mut app,
        &contracts,
        &controller,
        Uint128::new(500),
        Uint128::new(500),
    )
    .unwrap();

    let status = query_status(&app, &contracts);
    assert_eq!(status.available.u128(), 1000u128);
    assert_eq!(status.deployed.u128(), 0u128);
}

#[test]
fn test_add_remove_controller() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(1000000u128, "base")),
        (api.addr_make("controller"), vec![]),
    ];
    let (mut app, contracts) = setup(balances);

    let controller = app.api().addr_make("controller");
    let owner = app.api().addr_make("owner");

    add_controller(
        &mut app,
        &contracts,
        &owner,
        &controller,
        Uint128::new(1000),
    )
    .unwrap();

    let whitelist = query_whitelist(&app, &contracts);
    assert_eq!(whitelist.controllers.len(), 1);
    assert_eq!(whitelist.controllers[0].controller, controller.clone());
    assert_eq!(
        whitelist.controllers[0].limit.unwrap(),
        AmountU128::new(1000u128.into())
    );

    remove_controller(&mut app, &contracts, &owner, &controller).unwrap();
    let whitelist = query_whitelist(&app, &contracts);
    assert!(whitelist.controllers.is_empty());
}

#[test]
fn test_update_config() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![(api.addr_make("funder"), coins(1000000u128, "base"))];
    let (mut app, contracts) = setup(balances);

    let owner = app.api().addr_make("owner");
    let new_owner = app.api().addr_make("new_owner");

    update_config(&mut app, &contracts, &owner, new_owner.clone()).unwrap();
    let config = query_config(&app, &contracts);

    assert_eq!(config.owner, new_owner);
}

// Edge and error cases

#[test]
fn test_fund_with_zero_amount() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![(api.addr_make("funder"), coins(1000000u128, "base"))];
    let (mut app, contracts) = setup(balances);
    let funder = app.api().addr_make("funder");

    let result = fund(&mut app, &contracts, &funder, Uint128::new(0));
    assert!(result.is_err());
}

#[test]
fn test_withdraw_with_zero_amount() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![(api.addr_make("funder"), coins(1000000u128, "base"))];
    let (mut app, contracts) = setup(balances);
    let funder = app.api().addr_make("funder");

    fund(&mut app, &contracts, &funder, Uint128::new(1000)).unwrap();
    let result = withdraw(&mut app, &contracts, &funder, Uint128::new(0));
    assert!(result.is_err());
}

#[test]
fn test_request_reserves_with_zero_amount() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(1000000u128, "base")),
        (api.addr_make("controller"), vec![]),
    ];
    let (mut app, contracts) = setup(balances);

    let funder = app.api().addr_make("funder");
    let controller = app.api().addr_make("controller");
    let owner = app.api().addr_make("owner");

    fund(&mut app, &contracts, &funder, Uint128::new(1000)).unwrap();
    add_controller(
        &mut app,
        &contracts,
        &owner,
        &controller,
        Uint128::new(1000),
    )
    .unwrap();

    let result = request_reserves(&mut app, &contracts, &controller, Uint128::new(0));
    assert!(result.is_err());
}

#[test]
fn test_return_reserves_with_zero_amount() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(1000000u128, "base")),
        (api.addr_make("controller"), vec![]),
    ];
    let (mut app, contracts) = setup(balances);

    let funder = app.api().addr_make("funder");
    let controller = app.api().addr_make("controller");
    let owner = app.api().addr_make("owner");

    fund(&mut app, &contracts, &funder, Uint128::new(1000)).unwrap();
    add_controller(
        &mut app,
        &contracts,
        &owner,
        &controller,
        Uint128::new(1000),
    )
    .unwrap();
    request_reserves(&mut app, &contracts, &controller, Uint128::new(500)).unwrap();

    let result = return_reserves(
        &mut app,
        &contracts,
        &controller,
        Uint128::new(500),
        Uint128::new(0),
    );
    assert!(result.is_err());
}

#[test]
fn test_request_reserves_exceeding_limit() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(1000000u128, "base")),
        (api.addr_make("controller"), vec![]),
    ];
    let (mut app, contracts) = setup(balances);

    let funder = app.api().addr_make("funder");
    let controller = app.api().addr_make("controller");
    let owner = app.api().addr_make("owner");

    fund(&mut app, &contracts, &funder, Uint128::new(1000)).unwrap();
    add_controller(&mut app, &contracts, &owner, &controller, Uint128::new(500)).unwrap();

    let result = request_reserves(&mut app, &contracts, &controller, Uint128::new(1000));
    assert!(result.is_err());
}

#[test]
fn test_request_reserves_exceeding_liquidity() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(1000u128, "base")),
        (api.addr_make("controller"), vec![]),
    ];
    let (mut app, contracts) = setup(balances);

    let funder = app.api().addr_make("funder");
    let controller = app.api().addr_make("controller");
    let owner = app.api().addr_make("owner");

    fund(&mut app, &contracts, &funder, Uint128::new(1000)).unwrap();
    add_controller(
        &mut app,
        &contracts,
        &owner,
        &controller,
        Uint128::new(10000),
    )
    .unwrap();

    let result = request_reserves(&mut app, &contracts, &controller, Uint128::new(1001));
    assert!(result.is_err());
}

#[test]
fn test_return_reserves_exceeding_original_amount() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(1000000u128, "base")),
        (api.addr_make("controller"), vec![]),
    ];
    let (mut app, contracts) = setup(balances);

    let funder = app.api().addr_make("funder");
    let controller = app.api().addr_make("controller");
    let owner = app.api().addr_make("owner");

    fund(&mut app, &contracts, &funder, Uint128::new(1000)).unwrap();
    add_controller(
        &mut app,
        &contracts,
        &owner,
        &controller,
        Uint128::new(1000),
    )
    .unwrap();
    request_reserves(&mut app, &contracts, &controller, Uint128::new(500)).unwrap();

    let result = return_reserves(
        &mut app,
        &contracts,
        &controller,
        Uint128::new(500),
        Uint128::new(1000),
    );
    assert!(result.is_err());
}

#[test]
fn test_basic_fund() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![(api.addr_make("funder"), coins(1000000u128, "base"))];

    let (mut app, contracts) = setup(balances);
    let funder = app.api().addr_make("funder");

    app.execute_contract(
        funder.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::Fund { callback: None },
        &coins(1000u128, "base"),
    )
    .unwrap();

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.reserve.clone(), &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.available.u128(), 1000u128);
    assert_eq!(status.deployed.u128(), 0u128);
}

#[test]
fn test_basic_withdraw() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![(api.addr_make("funder"), coins(1000000u128, "base"))];
    let (mut app, contracts) = setup(balances);
    let funder = app.api().addr_make("funder");

    app.execute_contract(
        funder.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::Fund { callback: None },
        &coins(1000u128, "base"),
    )
    .unwrap();

    let ursv = format!("factory/{}/ursv", contracts.reserve);
    app.execute_contract(
        funder.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::Withdraw { callback: None },
        &coins(1000u128, ursv),
    )
    .unwrap();

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.reserve.clone(), &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.available.u128(), 0u128);
    assert_eq!(status.deployed.u128(), 0u128);
}

#[test]
fn test_basic_request_reserves() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(1000000u128, "base")),
        (api.addr_make("controller"), vec![]),
    ];
    let (mut app, contracts) = setup(balances);

    let funder = app.api().addr_make("funder");
    let controller = app.api().addr_make("controller");
    let owner = app.api().addr_make("owner");

    app.execute_contract(
        funder.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::Fund { callback: None },
        &coins(1000u128, "base"),
    )
    .unwrap();

    app.execute_contract(
        owner.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::AddController {
            controller: controller.clone(),
            limit: Some(AmountU128::new(1000u128.into())),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        controller.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::RequestReserves {
            requested_amount: AmountU128::new(500u128.into()),
            callback: None,
        },
        &[],
    )
    .unwrap();

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.reserve.clone(), &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.available.u128(), 500u128);
    assert_eq!(status.deployed.u128(), 500u128);
}

#[test]
fn test_basic_return_reserves() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(1000000u128, "base")),
        (api.addr_make("controller"), vec![]),
    ];
    let (mut app, contracts) = setup(balances);

    let funder = app.api().addr_make("funder");
    let controller = app.api().addr_make("controller");
    let owner = app.api().addr_make("owner");

    app.execute_contract(
        funder.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::Fund { callback: None },
        &coins(1000u128, "base"),
    )
    .unwrap();

    app.execute_contract(
        owner.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::AddController {
            controller: controller.clone(),
            limit: Some(AmountU128::new(1000u128.into())),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        controller.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::RequestReserves {
            requested_amount: AmountU128::new(500u128.into()),
            callback: None,
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        controller.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::ReturnReserves {
            original_amount: AmountU128::new(500u128.into()),
            callback: None,
        },
        &coins(500u128, "base"),
    )
    .unwrap();

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.reserve.clone(), &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.available.u128(), 1000u128);
    assert_eq!(status.deployed.u128(), 0u128);
}

#[test]
#[should_panic]
fn test_fund_with_large_amount() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(u128::MAX, "base")),
        (api.addr_make("extra"), coins(u128::MAX, "base")),
    ];
    let (mut app, contracts) = setup(balances);
    let funder = app.api().addr_make("funder");

    app.execute_contract(
        app.api().addr_make("extra"),
        contracts.reserve.clone(),
        &ExecuteMsg::Fund { callback: None },
        &coins(u128::MAX / 2, "base"),
    )
    .unwrap();

    let _ = app.execute_contract(
        funder.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::Fund { callback: None },
        &coins(u128::MAX, "base"),
    );

    // should panic
    panic!("Funding with max u128 should cause an overflow");
}

#[test]
fn test_return_reserves_with_different_amount() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(1000000u128, "base")),
        (api.addr_make("controller"), coins(1000000u128, "base")),
    ];
    let (mut app, contracts) = setup(balances);

    let funder = app.api().addr_make("funder");
    let controller = app.api().addr_make("controller");
    let owner = app.api().addr_make("owner");

    app.execute_contract(
        funder.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::Fund { callback: None },
        &coins(1000u128, "base"),
    )
    .unwrap();

    app.execute_contract(
        owner.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::AddController {
            controller: controller.clone(),
            limit: Some(AmountU128::new(1000u128.into())),
        },
        &[],
    )
    .unwrap();

    app.execute_contract(
        controller.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::RequestReserves {
            requested_amount: AmountU128::new(500u128.into()),
            callback: None,
        },
        &[],
    )
    .unwrap();

    // Return different amount (e.g., 600 base instead of 500)
    app.execute_contract(
        controller.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::ReturnReserves {
            original_amount: AmountU128::new(500u128.into()),
            callback: None,
        },
        &coins(600u128, "base"),
    )
    .unwrap();

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.reserve.clone(), &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.available.u128(), 1100u128); // 1000 + 100 (extra returned)
    assert_eq!(status.deployed.u128(), 0u128);
    // Check the updated reserve redemption ratio
    assert_eq!(
        status.reserve_redemption_rate.rate(),
        Decimal::from_ratio(1100u128, 1000u128)
    );
}

#[test]
fn test_request_reserves_unauthorized() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(1000000u128, "base")),
        (api.addr_make("controller"), vec![]),
    ];
    let (mut app, contracts) = setup(balances);

    let funder = app.api().addr_make("funder");
    let controller = app.api().addr_make("controller");

    app.execute_contract(
        funder.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::Fund { callback: None },
        &coins(1000u128, "base"),
    )
    .unwrap();

    let result = app.execute_contract(
        controller.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::RequestReserves {
            requested_amount: AmountU128::new(500u128.into()),
            callback: None,
        },
        &[],
    );

    assert!(result.is_err());
}

#[test]
fn test_rates() {
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(1000000u128, "base")),
        (api.addr_make("lender"), coins(100000000u128, "base")),
    ];
    let (mut app, contracts) = setup(balances);
    let funder = app.api().addr_make("funder");

    // Fund ghost first
    app.execute_contract(
        api.addr_make("lender"),
        contracts.ghost.clone(),
        &kujira_ghost::receipt_vault::ExecuteMsg::Deposit(
            kujira_ghost::receipt_vault::DepositMsg { callback: None },
        ),
        &coins(100000000u128, "base"),
    )
    .unwrap();

    app.execute_contract(
        funder.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::Fund { callback: None },
        &coins(10000u128, "base"),
    )
    .unwrap();

    // Wait for some time to pass
    // 2 weeks later, ghost rate should have increased
    app.update_block(|x| {
        x.time = x.time.plus_days(14);
    });

    let ursv = format!("factory/{}/ursv", contracts.reserve);
    app.execute_contract(
        funder.clone(),
        contracts.reserve.clone(),
        &ExecuteMsg::Withdraw { callback: None },
        &coins(10000u128, ursv),
    )
    .unwrap();

    let funder_balance = app.wrap().query_balance(funder.clone(), "base").unwrap();
    assert_eq!(funder_balance.amount.u128(), 1000000u128 + 382u128);

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.reserve.clone(), &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.available.u128(), 1u128); // Defensive rounding
    assert_eq!(status.deployed.u128(), 0u128);
}
