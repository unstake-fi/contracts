use std::str::FromStr;

use cosmwasm_std::{coins, to_json_binary, Addr, Coin, Decimal, Uint128};
use cw_multi_test::{ContractWrapper, Executor};
use kujira::{Denom, HumanPrice};
use kujira_ghost::common::OracleType;
use kujira_rs_testing::mock::{mock_app, CustomApp};
use unstake::{
    adapter::Contract,
    controller::{ExecuteMsg, OfferResponse, QueryMsg},
};

struct Contracts {
    pub ghost: Addr,
    pub provider: Addr,
    pub controller: Addr,
}

fn setup(balances: Vec<(Addr, Vec<Coin>)>) -> (CustomApp, Contracts) {
    let mut app = mock_app(balances);
    let delegate_code = ContractWrapper::new(
        unstake_delegate::contract::execute,
        unstake_delegate::contract::instantiate,
        unstake_delegate::contract::query,
    );
    let controller_code = ContractWrapper::new(
        crate::contract::execute,
        crate::contract::instantiate,
        crate::contract::query,
    );
    let ghost_code = ContractWrapper::new(
        crate::testing::ghost::execute,
        crate::testing::ghost::instantiate,
        crate::testing::ghost::query,
    );

    let provider_code = ContractWrapper::new(
        crate::testing::provider::execute,
        crate::testing::provider::instantiate,
        crate::testing::provider::query,
    );

    let delegate_code_id = app.store_code(Box::new(delegate_code));
    let controller_code_id = app.store_code(Box::new(controller_code));
    let ghost_code_id = app.store_code(Box::new(ghost_code));
    let provider_code_id = app.store_code(Box::new(provider_code));

    let vault_address = app
        .instantiate_contract(
            ghost_code_id,
            Addr::unchecked("ghost"),
            &kujira_ghost::receipt_vault::InstantiateMsg {
                owner: Addr::unchecked("ghost-owner"),
                denom: Denom::from("quote"),
                oracle: OracleType::Static(HumanPrice::from(Decimal::one())),
                decimals: 6,
                receipt_denom: "urcpt".to_string(),
                debt_token_denom: "udebt".to_string(),
                denom_creation_fee: Uint128::zero(),
                utilization_to_rate: vec![],
            },
            &vec![],
            "ghost",
            None,
        )
        .unwrap();

    let provider_address = app
        .instantiate_contract(
            provider_code_id,
            Addr::unchecked("provider"),
            &(),
            &vec![],
            "provider",
            None,
        )
        .unwrap();

    let redemption_rate_query =
        to_json_binary(&unstake::adapter::ContractQueryMsg::State {}).unwrap();

    let unbond_start_msg =
        to_json_binary(&crate::testing::provider::ExecuteMsg::QueueUnbond {}).unwrap();

    let unbond_end_msg =
        to_json_binary(&crate::testing::provider::ExecuteMsg::WithdrawUnbonded {}).unwrap();

    let controller_address = app
        .instantiate_contract(
            controller_code_id,
            Addr::unchecked("instantiator"),
            &unstake::controller::InstantiateMsg {
                owner: Addr::unchecked("owner"),
                protocol_fee: Decimal::zero(),
                delegate_code_id,
                vault_address: vault_address.clone(),
                ask_denom: Denom::from("base"),
                offer_denom: Denom::from("quote"),
                adapter: unstake::adapter::Adapter::Contract(Contract {
                    address: provider_address.clone(),
                    redemption_rate_query,
                    unbond_start_msg,
                    unbond_end_msg,
                }),
                // 2 weeks
                unbonding_duration: 2 * 7 * 24 * 60 * 60,
                // 3%
                min_rate: Decimal::from_str("0.03").unwrap(),
            },
            &vec![],
            "controller",
            None,
        )
        .unwrap();
    (
        app,
        Contracts {
            ghost: vault_address,
            provider: provider_address,
            controller: controller_address,
        },
    )
}

#[test]
fn instantiate() {
    setup(vec![]);
}

#[test]
fn quote_initial() {
    // Check that when the contract is new, and there is no reserve fund, the quoted rate uses the max rate from the vault
    let (app, contracts) = setup(vec![]);
    let quote: OfferResponse = app
        .wrap()
        .query_wasm_smart(
            contracts.controller,
            &QueryMsg::Offer {
                amount: Uint128::from(10000u128),
            },
        )
        .unwrap();
    // Mock redemption rate of 1.07375
    // current interest rate: 100%
    // Max interest rate of 300%
    // Default 2 week unbonding

    // 31,536,000 seconds in a year
    // 1,209,600 in 2 weeks
    // 0.03835616438 of the max interest rate
    // 0.1150684931 interest
    // List price 10737
    // Interest amount 1234
    // Offer amount 10737 - 1234 = 9,503
    assert_eq!(quote.amount, Uint128::from(9503u128));
    assert_eq!(quote.fee, Uint128::from(1234u128));
}

#[test]
fn quote_reserve_clamped() {
    // Same as above, but with a small amount of reserve allocated that will be entirely consumed
    let balances = vec![(Addr::unchecked("funder"), coins(100000000u128, "quote"))];
    let (mut app, contracts) = setup(balances);
    app.execute_contract(
        Addr::unchecked("funder"),
        contracts.controller.clone(),
        &ExecuteMsg::Fund {},
        &coins(200u128, "quote"),
    )
    .unwrap();

    let quote: OfferResponse = app
        .wrap()
        .query_wasm_smart(
            contracts.controller,
            &QueryMsg::Offer {
                amount: Uint128::from(10000u128),
            },
        )
        .unwrap();

    // Mock redemption rate of 1.07375
    // current interest rate: 100%
    // Max interest rate of 300%
    // Default 2 week unbonding

    // 31,536,000 seconds in a year
    // 1,209,600 in 2 weeks
    // 0.03835616438 of the max interest rate
    // 0.1150684931 interest
    // List price 10737
    // Interest amount 1234
    // Available reserve = 200
    // Offer amount 10737 - 1034 = 9,703
    assert_eq!(quote.amount, Uint128::from(9703u128));
    assert_eq!(quote.fee, Uint128::from(1034u128));
}

#[test]
fn quote_unclamped() {
    // Quote where we have plenty of reserves, and the current rate is higher than the minimum rate
    let balances = vec![(Addr::unchecked("funder"), coins(100000000u128, "quote"))];
    let (mut app, contracts) = setup(balances);
    app.execute_contract(
        Addr::unchecked("funder"),
        contracts.controller.clone(),
        &ExecuteMsg::Fund {},
        &coins(20000u128, "quote"),
    )
    .unwrap();

    let quote: OfferResponse = app
        .wrap()
        .query_wasm_smart(
            contracts.controller,
            &QueryMsg::Offer {
                amount: Uint128::from(10000u128),
            },
        )
        .unwrap();

    // Mock redemption rate of 1.07375
    // current interest rate: 100%
    // Max interest rate of 300%
    // Default 2 week unbonding

    // 31,536,000 seconds in a year
    // 1,209,600 in 2 weeks
    // 0.03835616438 of the current interest rate
    // 0.03835616438 interest
    // List price 10737, interest 411
    // Offer amount 10737 - 411 = 9,703
    assert_eq!(quote.amount, Uint128::from(10326u128));
    assert_eq!(quote.fee, Uint128::from(411u128));
}

#[test]
fn quote_min_rate_clamped() {
    // Quote where we have plenty of reserves, and the minimum rate is highter than the current rate
    let balances = vec![(Addr::unchecked("funder"), coins(100000000u128, "quote"))];
    let (mut app, contracts) = setup(balances);
    app.execute_contract(
        Addr::unchecked("funder"),
        contracts.controller.clone(),
        &ExecuteMsg::Fund {},
        &coins(20000u128, "quote"),
    )
    .unwrap();

    app.execute_contract(
        Addr::unchecked("owner"),
        contracts.controller.clone(),
        &ExecuteMsg::UpdateBroker {
            vault: None,
            min_rate: Some(Decimal::from_str("1.1").unwrap()),
            duration: None,
        },
        &vec![],
    )
    .unwrap();

    let quote: OfferResponse = app
        .wrap()
        .query_wasm_smart(
            contracts.controller,
            &QueryMsg::Offer {
                amount: Uint128::from(10000u128),
            },
        )
        .unwrap();

    // Mock redemption rate of 1.07375
    // current interest rate: 100%
    // min interest rate 110%
    // Max interest rate of 300%
    // Default 2 week unbonding

    // 31,536,000 seconds in a year
    // 1,209,600 in 2 weeks
    // 0.03835616438 of the current interest rate
    // 0.04219178082 interest
    // List price 10737, interest 452
    // Offer amount 10737 - 452 = 10,285
    assert_eq!(quote.amount, Uint128::from(10285u128));
    assert_eq!(quote.fee, Uint128::from(452u128));
}
