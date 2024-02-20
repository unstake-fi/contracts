use std::str::FromStr;

use cosmwasm_std::{coin, coins, Addr, Coin, Decimal, Uint128};
use cw_multi_test::{ContractWrapper, Executor};
use kujira::{fee_address, Denom, HumanPrice};
use kujira_ghost::common::OracleType;
use kujira_rs_testing::{
    api::MockApiBech32,
    mock::{mock_app, CustomApp},
};
use unstake::controller::{DelegatesResponse, ExecuteMsg, OfferResponse, QueryMsg, StatusResponse};

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
            app.api().addr_make("ghost"),
            &kujira_ghost::receipt_vault::InstantiateMsg {
                owner: app.api().addr_make("ghost-owner"),
                denom: Denom::from("quote"),
                oracle: OracleType::Static(HumanPrice::from(Decimal::one())),
                decimals: 6,
                denom_creation_fee: Uint128::zero(),
                utilization_to_curve: vec![],
            },
            &vec![],
            "ghost",
            None,
        )
        .unwrap();

    let provider_address = app
        .instantiate_contract(
            provider_code_id,
            app.api().addr_make("provider"),
            &(),
            &vec![],
            "provider",
            None,
        )
        .unwrap();

    let controller_address = app
        .instantiate_contract(
            controller_code_id,
            app.api().addr_make("instantiator"),
            &unstake::controller::InstantiateMsg {
                owner: app.api().addr_make("owner"),
                protocol_fee: Decimal::from_str("0.25").unwrap(),
                protocol_fee_address: fee_address(),
                delegate_code_id,
                vault_address: vault_address.clone(),
                ask_denom: Denom::from("base"),
                offer_denom: Denom::from("quote"),
                adapter: unstake::adapter::Adapter::Eris(provider_address.clone().into()),
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

fn query_balances(app: &CustomApp, address: Addr) -> Vec<Coin> {
    app.wrap().query_all_balances(address).unwrap()
}

#[test]
fn instantiate() {
    setup(vec![]);
}

#[test]
fn quote_initial() {
    // Check that when the contract is new, and there is no reserve fund, the quoted rate uses the max rate from the vault
    let (app, contracts) = setup(vec![]);

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller.clone(), &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.reserve_available, Uint128::zero());
    assert_eq!(status.reserve_deployed, Uint128::zero());
    assert_eq!(status.total_base, Uint128::zero());
    assert_eq!(status.total_quote, Uint128::zero());

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
    assert_eq!(quote.amount, Uint128::from(9501u128));
    assert_eq!(quote.fee, Uint128::from(1236u128));
}

#[test]
fn quote_reserve_clamped() {
    let api = MockApiBech32::new("kujira");

    // Same as above, but with a small amount of reserve allocated that will be entirely consumed
    let balances = vec![(api.addr_make("funder"), coins(100000000u128, "quote"))];
    let (mut app, contracts) = setup(balances);
    app.execute_contract(
        app.api().addr_make("funder"),
        contracts.controller.clone(),
        &ExecuteMsg::Fund {},
        &coins(200u128, "quote"),
    )
    .unwrap();

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller.clone(), &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.reserve_available, Uint128::from(200u128));
    assert_eq!(status.reserve_deployed, Uint128::zero());
    assert_eq!(status.total_base, Uint128::zero());
    assert_eq!(status.total_quote, Uint128::zero());

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
    assert_eq!(quote.amount, Uint128::from(9701u128));
    assert_eq!(quote.fee, Uint128::from(1036u128));
}

#[test]
fn quote_unclamped() {
    // Quote where we have plenty of reserves, and the current rate is higher than the minimum rate
    let api = MockApiBech32::new("kujira");

    let balances = vec![(api.addr_make("funder"), coins(100000000u128, "quote"))];
    let (mut app, contracts) = setup(balances);
    app.execute_contract(
        app.api().addr_make("funder"),
        contracts.controller.clone(),
        &ExecuteMsg::Fund {},
        &coins(20000u128, "quote"),
    )
    .unwrap();

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller.clone(), &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.reserve_available, Uint128::from(20000u128));
    assert_eq!(status.reserve_deployed, Uint128::zero());
    assert_eq!(status.total_base, Uint128::zero());
    assert_eq!(status.total_quote, Uint128::zero());

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
    assert_eq!(quote.amount, Uint128::from(10325u128));
    assert_eq!(quote.fee, Uint128::from(412u128));
}

#[test]
fn quote_min_rate_clamped() {
    // Quote where we have plenty of reserves, and the minimum rate is highter than the current rate
    let api = MockApiBech32::new("kujira");

    let balances = vec![(api.addr_make("funder"), coins(100000000u128, "quote"))];
    let (mut app, contracts) = setup(balances);
    app.execute_contract(
        app.api().addr_make("funder"),
        contracts.controller.clone(),
        &ExecuteMsg::Fund {},
        &coins(20000u128, "quote"),
    )
    .unwrap();

    app.execute_contract(
        app.api().addr_make("owner"),
        contracts.controller.clone(),
        &ExecuteMsg::UpdateBroker {
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
    assert_eq!(quote.amount, Uint128::from(10283u128));
    assert_eq!(quote.fee, Uint128::from(454u128));
}

#[test]
fn execute_offer() {
    // Quote where we have plenty of reserves, and the minimum rate is highter than the current rate
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(100000000u128, "quote")),
        (api.addr_make("unstaker"), coins(10000u128, "base")),
        (api.addr_make("lender"), coins(100000000u128, "quote")),
    ];
    let (mut app, contracts) = setup(balances);

    app.execute_contract(
        api.addr_make("funder"),
        contracts.controller.clone(),
        &ExecuteMsg::Fund {},
        &coins(20000u128, "quote"),
    )
    .unwrap();

    app.execute_contract(
        api.addr_make("lender"),
        contracts.ghost.clone(),
        &kujira_ghost::receipt_vault::ExecuteMsg::Deposit(
            kujira_ghost::receipt_vault::DepositMsg { callback: None },
        ),
        &coins(100000000u128, "quote"),
    )
    .unwrap();

    let amount = Uint128::from(10000u128);

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller.clone(), &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.reserve_available, Uint128::from(20000u128));
    assert_eq!(status.reserve_deployed, Uint128::zero());
    assert_eq!(status.total_base, Uint128::zero());
    assert_eq!(status.total_quote, Uint128::zero());

    app.execute_contract(
        api.addr_make("unstaker"),
        contracts.controller.clone(),
        &ExecuteMsg::Unstake {
            callback: None,
            max_fee: amount,
        },
        &coins(10000u128, "base"),
    )
    .unwrap();

    let delegates: DelegatesResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller.clone(), &QueryMsg::Delegates {})
        .unwrap();
    assert_eq!(delegates.delegates.len(), 1);
    let (delegate, _) = delegates.delegates[0].clone();

    let unstaker_balances = query_balances(&app, api.addr_make("unstaker"));
    let controller_balances = query_balances(&app, contracts.controller.clone());
    let delegate_balances = query_balances(&app, delegate);
    let provider_balances = query_balances(&app, contracts.provider);
    let ghost_balances = query_balances(&app, contracts.ghost.clone());

    // 10000 unstaked
    // 10326 returned to user
    // 411 in fees
    // reserve allocation (823) must be sent to delegate
    // debt_rate 1.12
    // debt_tokens 11566
    // ghots depost 100000000 - 10326 = 99989674

    // unstaker gets their money, less the fees
    assert_eq!(unstaker_balances, coins(10325u128, "quote"));
    // remainder of reserve left on controller
    assert_eq!(controller_balances, coins(19176u128, "quote"));
    // delegate has the debt tokens, and reserve allocation
    assert_eq!(
        delegate_balances,
        vec![
            coin(9219u128, format!("factory/{}/udebt", contracts.ghost)),
            coin(824u128, "quote")
        ]
    );
    // Provider should have received the base for unbonding
    assert_eq!(provider_balances, coins(10000u128, "base"));

    // And ghost should have the borrowed amount deducted
    assert_eq!(ghost_balances, coins(100000000u128 - 10325u128, "quote"));

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller, &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.reserve_available, Uint128::from(20000u128 - 824));
    assert_eq!(status.reserve_deployed, Uint128::from(824u128));
    assert_eq!(status.total_base, Uint128::from(10000u128));
    assert_eq!(status.total_quote, Uint128::zero());
}

#[test]
fn execute_unfunded_offer() {
    // Quote where we no plenty of reserves
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("unstaker"), coins(10000u128, "base")),
        (api.addr_make("lender"), coins(100000000u128, "quote")),
    ];
    let (mut app, contracts) = setup(balances);

    app.execute_contract(
        api.addr_make("lender"),
        contracts.ghost.clone(),
        &kujira_ghost::receipt_vault::ExecuteMsg::Deposit(
            kujira_ghost::receipt_vault::DepositMsg { callback: None },
        ),
        &coins(100000000u128, "quote"),
    )
    .unwrap();

    let amount = Uint128::from(10000u128);

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller.clone(), &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.reserve_available, Uint128::zero());
    assert_eq!(status.reserve_deployed, Uint128::zero());
    assert_eq!(status.total_base, Uint128::zero());
    assert_eq!(status.total_quote, Uint128::zero());

    app.execute_contract(
        api.addr_make("unstaker"),
        contracts.controller.clone(),
        &ExecuteMsg::Unstake {
            callback: None,
            max_fee: amount,
        },
        &coins(10000u128, "base"),
    )
    .unwrap();

    let delegates: DelegatesResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller.clone(), &QueryMsg::Delegates {})
        .unwrap();
    assert_eq!(delegates.delegates.len(), 1);
    let (delegate, _) = delegates.delegates[0].clone();

    let unstaker_balances = query_balances(&app, api.addr_make("unstaker"));
    let controller_balances = query_balances(&app, contracts.controller.clone());
    let delegate_balances = query_balances(&app, delegate);
    let provider_balances = query_balances(&app, contracts.provider);
    let ghost_balances = query_balances(&app, contracts.ghost.clone());

    // unstaker gets their money, less the fees
    assert_eq!(unstaker_balances, coins(9501u128, "quote"));
    // remainder of reserve left on controller
    assert_eq!(controller_balances, vec![]);
    // delegate has the debt tokens, and no reserve allocation
    assert_eq!(
        delegate_balances,
        coins(8484u128, format!("factory/{}/udebt", contracts.ghost)),
    );
    // Provider should have received the base for unbonding
    assert_eq!(provider_balances, coins(10000u128, "base"));

    // And ghost should have the borrowed amount deducted
    assert_eq!(ghost_balances, coins(100000000u128 - 9501u128, "quote"));

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller, &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.reserve_available, Uint128::zero());
    assert_eq!(status.reserve_deployed, Uint128::zero());
    assert_eq!(status.total_base, Uint128::from(10000u128));
    assert_eq!(status.total_quote, Uint128::zero());
}

#[test]
fn close_offer() {
    // Quote where we have plenty of reserves, and the minimum rate is highter than the current rate
    // The interest rate on ghost will be unchanged, so this will emulate a "perfect" unstake - ie the
    // rate charged is exactly what is paid
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(100000000u128, "quote")),
        (api.addr_make("unstaker"), coins(10000u128, "base")),
        (api.addr_make("lender"), coins(100000000u128, "quote")),
    ];
    let (mut app, contracts) = setup(balances);

    app.execute_contract(
        api.addr_make("funder"),
        contracts.controller.clone(),
        &ExecuteMsg::Fund {},
        &coins(20000u128, "quote"),
    )
    .unwrap();

    // Make sure that the provider has enough tokens to return once unbonding is complete
    app.send_tokens(
        api.addr_make("funder"),
        contracts.provider.clone(),
        &coins(500000u128, "quote"),
    )
    .unwrap();

    app.execute_contract(
        api.addr_make("lender"),
        contracts.ghost.clone(),
        &kujira_ghost::receipt_vault::ExecuteMsg::Deposit(
            kujira_ghost::receipt_vault::DepositMsg { callback: None },
        ),
        &coins(100000000u128, "quote"),
    )
    .unwrap();

    let amount = Uint128::from(10000u128);

    app.execute_contract(
        api.addr_make("unstaker"),
        contracts.controller.clone(),
        &ExecuteMsg::Unstake {
            callback: None,
            max_fee: amount,
        },
        &coins(10000u128, "base"),
    )
    .unwrap();

    let delegates: DelegatesResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller.clone(), &QueryMsg::Delegates {})
        .unwrap();
    assert_eq!(delegates.delegates.len(), 1);
    let (delegate, _) = delegates.delegates[0].clone();

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller.clone(), &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.reserve_available, Uint128::from(20000u128 - 824));
    assert_eq!(status.reserve_deployed, Uint128::from(824u128));
    assert_eq!(status.total_base, Uint128::from(10000u128));
    assert_eq!(status.total_quote, Uint128::zero());

    // 2 weeks later, ghost debt rate should have increased
    app.update_block(|x| {
        x.time = x.time.plus_days(14);
    });

    app.execute_contract(
        api.addr_make("random"),
        delegate.clone(),
        &unstake::delegate::ExecuteMsg::Complete {},
        &vec![],
    )
    .unwrap();

    let controller_balances = query_balances(&app, contracts.controller.clone());
    let delegate_balances = query_balances(&app, delegate);
    let provider_balances = query_balances(&app, contracts.provider);
    let ghost_balances = query_balances(&app, contracts.ghost.clone());

    // despite the rate being as-predicted, this unstake will generate a profit.
    // this is because the unstake fee - in this case 100% APR over 2 weeks = 3.8356%
    // is charged on the amount of quote asset returned, but in order to honour this
    // unbonding, we actually only need to borrow 100% - 3.8356% of the unbonded amout.

    // We can calculate our way around this when quoting, but it's a nice extra bit of revenue
    // for the protocol

    // So we'll pay 3.8356 on 10326 = 396,
    // but have an allocated fee of 3.8356 on the total value = 10737 * 3.8356 = 411
    // so we have an excess profit here of 411 - 396 = 15
    // of that profit, 25% is protocol_fee, so 3
    assert_eq!(controller_balances, coins(20012u128, "quote"));

    // delegate should now be empty
    assert_eq!(delegate_balances, vec![]);

    // Provider should have received the base for unbonding, plus the surplus from the original funding
    // (500,000 - (10000 * 1.07375))
    assert_eq!(
        provider_balances,
        vec![coin(10000u128, "base"), coin(489263u128, "quote")]
    );

    // And ghost should have the borrowed amount returned with interest
    // 10326 over 2 weeks at 100% = 1 / 26 = 397.
    assert_eq!(ghost_balances, coins(100000397u128, "quote"));

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller, &QueryMsg::Status {})
        .unwrap();

    // Remainder of the profit goes onto the reserve
    assert_eq!(status.reserve_available, Uint128::from(20000u128 + 12));
    assert_eq!(status.reserve_deployed, Uint128::zero());
    assert_eq!(status.total_base, Uint128::from(10000u128));
    // 10000 * 1.07375 for returned amount
    assert_eq!(status.total_quote, Uint128::from(10737u128));
}

#[test]
fn close_early_offer() {
    // Make sure that we bail if the offer attempts to close early, and there's nothing returned
    // from the provider to repay ghost
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(100000000u128, "quote")),
        (api.addr_make("unstaker"), coins(10000u128, "base")),
        (api.addr_make("lender"), coins(100000000u128, "quote")),
    ];
    let (mut app, contracts) = setup(balances);

    app.execute_contract(
        api.addr_make("funder"),
        contracts.controller.clone(),
        &ExecuteMsg::Fund {},
        &coins(20000u128, "quote"),
    )
    .unwrap();

    // Make sure that the provider has enough tokens to return once unbonding is complete
    app.send_tokens(
        api.addr_make("funder"),
        contracts.provider.clone(),
        &coins(500000u128, "quote"),
    )
    .unwrap();

    app.execute_contract(
        api.addr_make("lender"),
        contracts.ghost.clone(),
        &kujira_ghost::receipt_vault::ExecuteMsg::Deposit(
            kujira_ghost::receipt_vault::DepositMsg { callback: None },
        ),
        &coins(100000000u128, "quote"),
    )
    .unwrap();

    let amount = Uint128::from(10000u128);

    app.execute_contract(
        api.addr_make("unstaker"),
        contracts.controller.clone(),
        &ExecuteMsg::Unstake {
            callback: None,
            max_fee: amount,
        },
        &coins(10000u128, "base"),
    )
    .unwrap();

    let delegates: DelegatesResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller.clone(), &QueryMsg::Delegates {})
        .unwrap();
    assert_eq!(delegates.delegates.len(), 1);
    let (delegate, _) = delegates.delegates[0].clone();

    // 2 weeks later, ghost debt rate should have increased
    app.update_block(|x| {
        x.time = x.time.plus_days(13);
    });

    app.execute_contract(
        api.addr_make("random"),
        delegate.clone(),
        &unstake::delegate::ExecuteMsg::Complete {},
        &vec![],
    )
    .unwrap_err();
}

#[test]
fn close_losing_offer() {
    // Now let's finally test a loss-making completion
    // Instead of updating the ghost rate, we will just make it complete the unbonding a week too late
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(100000000u128, "quote")),
        (api.addr_make("unstaker"), coins(10000u128, "base")),
        (api.addr_make("lender"), coins(100000000u128, "quote")),
    ];
    let (mut app, contracts) = setup(balances);

    app.execute_contract(
        api.addr_make("funder"),
        contracts.controller.clone(),
        &ExecuteMsg::Fund {},
        &coins(20000u128, "quote"),
    )
    .unwrap();

    // Make sure that the provider has enough tokens to return once unbonding is complete
    app.send_tokens(
        api.addr_make("funder"),
        contracts.provider.clone(),
        &coins(500000u128, "quote"),
    )
    .unwrap();

    app.execute_contract(
        api.addr_make("lender"),
        contracts.ghost.clone(),
        &kujira_ghost::receipt_vault::ExecuteMsg::Deposit(
            kujira_ghost::receipt_vault::DepositMsg { callback: None },
        ),
        &coins(100000000u128, "quote"),
    )
    .unwrap();

    let amount = Uint128::from(10000u128);

    app.execute_contract(
        api.addr_make("unstaker"),
        contracts.controller.clone(),
        &ExecuteMsg::Unstake {
            callback: None,
            max_fee: amount,
        },
        &coins(10000u128, "base"),
    )
    .unwrap();

    let delegates: DelegatesResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller.clone(), &QueryMsg::Delegates {})
        .unwrap();
    assert_eq!(delegates.delegates.len(), 1);
    let (delegate, _) = delegates.delegates[0].clone();

    // 2 weeks later, ghost debt rate should have increased
    app.update_block(|x| {
        x.time = x.time.plus_days(21);
    });

    app.execute_contract(
        api.addr_make("random"),
        delegate.clone(),
        &unstake::delegate::ExecuteMsg::Complete {},
        &vec![],
    )
    .unwrap();

    let controller_balances = query_balances(&app, contracts.controller);
    let delegate_balances = query_balances(&app, delegate);
    let provider_balances = query_balances(&app, contracts.provider);
    let ghost_balances = query_balances(&app, contracts.ghost.clone());

    // 21 days will be a total of 5.7534% interest
    // So we'll pay 5.7534 on 10326 = 594,
    // but have an allocated fee of 3.8356 on the total value = 10737 * 3.8356 = 411
    // so we have a loss  here of 411 - 594 = -184 (rounding)
    assert_eq!(controller_balances, coins(19817u128, "quote"));

    // delegate should now be empty
    assert_eq!(delegate_balances, vec![]);

    // Provider should have received the base for unbonding, plus the surplus from the original funding
    // (500,000 - (10000 * 1.07375))
    assert_eq!(
        provider_balances,
        vec![coin(10000u128, "base"), coin(489263u128, "quote")]
    );

    // And ghost should have the borrowed amount returned with interest
    // 10326 over 3 weeks at 100% = 3 / 52 = 595.
    assert_eq!(ghost_balances, coins(100000595u128, "quote"));
}
