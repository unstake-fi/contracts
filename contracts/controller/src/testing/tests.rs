use std::str::FromStr;

use cosmwasm_std::{coin, coins, Addr, Coin, Decimal, Event, Uint128};
use cw_multi_test::{ContractWrapper, Executor};
use kujira::{fee_address, Denom, HumanPrice};
use kujira_ghost::common::OracleType;
use kujira_rs_testing::{
    api::MockApiBech32,
    mock::{mock_app, CustomApp},
};
use monetary::AmountU128;
use unstake::{
    controller::{DelegatesResponse, ExecuteMsg, OfferResponse, QueryMsg, StatusResponse},
    denoms::Base,
};

struct Contracts {
    pub ghost: Addr,
    pub provider: Addr,
    pub controller: Addr,
    pub reserve: Addr,
}

fn setup(
    balances: Vec<(Addr, Vec<Coin>)>,
    controller_limit: Option<AmountU128<Base>>,
) -> (CustomApp, Contracts) {
    let mut app = mock_app(balances);

    let delegate_code = ContractWrapper::new(
        unstake_delegate::contract::execute,
        unstake_delegate::contract::instantiate,
        unstake_delegate::contract::query,
    );
    let reserve_code = ContractWrapper::new(
        unstake_reserve::contract::execute,
        unstake_reserve::contract::instantiate,
        unstake_reserve::contract::query,
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
    let reserve_code_id = app.store_code(Box::new(reserve_code));
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

    let reserve_address = app
        .instantiate_contract(
            reserve_code_id,
            app.api().addr_make("reserve"),
            &unstake::reserve::InstantiateMsg {
                owner: app.api().addr_make("owner"),
                ghost_vault_addr: vault_address.clone(),
                base_denom: monetary::Denom::new("quote"),
            },
            &vec![],
            "reserve",
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
                reserve_address: reserve_address.clone(),
                ask_denom: monetary::Denom::new("base"),
                offer_denom: monetary::Denom::new("quote"),
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

    // Add controller to reserve
    app.execute_contract(
        app.api().addr_make("owner"),
        reserve_address.clone(),
        &unstake::reserve::ExecuteMsg::AddController {
            controller: controller_address.clone(),
            limit: controller_limit,
        },
        &vec![],
    )
    .unwrap();

    (
        app,
        Contracts {
            ghost: vault_address,
            provider: provider_address,
            controller: controller_address,
            reserve: reserve_address,
        },
    )
}

fn fund_reserve(app: &mut CustomApp, funder: Addr, reserve: Addr, amount: Uint128, denom: &str) {
    app.execute_contract(
        funder,
        reserve,
        &unstake::reserve::ExecuteMsg::Fund { callback: None },
        &coins(amount.u128(), denom),
    )
    .unwrap();
}

fn query_balances(app: &CustomApp, address: Addr) -> Vec<Coin> {
    app.wrap().query_all_balances(address).unwrap()
}
#[test]
fn instantiate() {
    setup(vec![], None);
}

#[test]
fn quote_initial() {
    // Check that when the contract is new, and there is no reserve fund, the quoted rate uses the max rate from the vault
    let (app, contracts) = setup(vec![], None);

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller.clone(), &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.total_base, AmountU128::zero());
    assert_eq!(status.total_quote, AmountU128::zero());

    let quote: OfferResponse = app
        .wrap()
        .query_wasm_smart(
            contracts.controller,
            &QueryMsg::Offer {
                amount: AmountU128::new(Uint128::from(10000u128)),
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
    assert_eq!(quote.amount, AmountU128::new(Uint128::from(9501u128)));
    assert_eq!(quote.fee, AmountU128::new(Uint128::from(1236u128)));
}

#[test]
fn quote_reserve_clamped() {
    let api = MockApiBech32::new("kujira");

    // Same as above, but with a small amount of reserve allocated that will be entirely consumed
    let balances = vec![(api.addr_make("funder"), coins(100000000u128, "quote"))];
    let (mut app, contracts) = setup(balances, None);
    fund_reserve(
        &mut app,
        api.addr_make("funder"),
        contracts.reserve.clone(),
        200u128.into(),
        "quote",
    );

    let quote: OfferResponse = app
        .wrap()
        .query_wasm_smart(
            contracts.controller,
            &QueryMsg::Offer {
                amount: AmountU128::new(Uint128::from(10000u128)),
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
    assert_eq!(quote.amount, AmountU128::new(Uint128::from(9701u128)));
    assert_eq!(quote.fee, AmountU128::new(Uint128::from(1036u128)));
}

#[test]
fn quote_unclamped() {
    // Quote where we have plenty of reserves, and the current rate is higher than the minimum rate
    let api = MockApiBech32::new("kujira");

    let balances = vec![(api.addr_make("funder"), coins(100000000u128, "quote"))];
    let (mut app, contracts) = setup(balances, None);
    fund_reserve(
        &mut app,
        api.addr_make("funder"),
        contracts.reserve.clone(),
        20000u128.into(),
        "quote",
    );

    let quote: OfferResponse = app
        .wrap()
        .query_wasm_smart(
            contracts.controller,
            &QueryMsg::Offer {
                amount: AmountU128::new(Uint128::from(10000u128)),
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
    assert_eq!(quote.amount, AmountU128::new(Uint128::from(10325u128)));
    assert_eq!(quote.fee, AmountU128::new(Uint128::from(412u128)));
}

#[test]
fn quote_min_rate_clamped() {
    // Quote where we have plenty of reserves, and the minimum rate is highter than the current rate
    let api = MockApiBech32::new("kujira");

    let balances = vec![(api.addr_make("funder"), coins(100000000u128, "quote"))];
    let (mut app, contracts) = setup(balances, None);
    fund_reserve(
        &mut app,
        api.addr_make("funder"),
        contracts.reserve.clone(),
        20000u128.into(),
        "quote",
    );

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
                amount: AmountU128::new(Uint128::from(10000u128)),
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
    assert_eq!(quote.amount, AmountU128::new(Uint128::from(10283u128)));
    assert_eq!(quote.fee, AmountU128::new(Uint128::from(454u128)));
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
    let (mut app, contracts) = setup(balances, None);
    fund_reserve(
        &mut app,
        api.addr_make("funder"),
        contracts.reserve.clone(),
        20000u128.into(),
        "quote",
    );

    app.execute_contract(
        api.addr_make("lender"),
        contracts.ghost.clone(),
        &kujira_ghost::receipt_vault::ExecuteMsg::Deposit(
            kujira_ghost::receipt_vault::DepositMsg { callback: None },
        ),
        &coins(100000000u128, "quote"),
    )
    .unwrap();

    let amount = AmountU128::new(Uint128::from(10000u128));

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
    let reserve_balances = query_balances(&app, contracts.reserve.clone());
    let delegate_balances = query_balances(&app, delegate);
    let provider_balances = query_balances(&app, contracts.provider);
    let ghost_balances = query_balances(&app, contracts.ghost.clone());

    // 10000 unstaked
    // 10325 returned to user
    // 411 in fees
    // reserve allocation (823)
    // debt_rate 1.12
    // (10326 - 823) / 1.12 debt tokens -> 8484
    // ghots depost 100000000 + 20000 (reserves) - (10325 - 823reserve allocation) = 99990498

    // unstaker gets their money, less the fees
    assert_eq!(unstaker_balances, coins(10325u128, "quote"));
    // remainder of reserve left on reserve
    assert_eq!(
        reserve_balances,
        coins(19176u128, format!("factory/{}/urcpt", contracts.ghost))
    );
    // delegate has the debt tokens, and reserve allocation
    assert_eq!(
        delegate_balances,
        vec![coin(8484u128, format!("factory/{}/udebt", contracts.ghost)),]
    );
    // Provider should have received the base for unbonding
    assert_eq!(provider_balances, coins(10000u128, "base"));

    // And ghost should have the borrowed amount deducted
    let ghost_starting = 100000000u128 + 20000u128;
    let borrow = 9501u128 + 823u128 + 1u128; // (borrow amount) + reserve withdrawn + 1 (rounding)
    assert_eq!(ghost_balances, coins(ghost_starting - borrow, "quote"));

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller, &QueryMsg::Status {})
        .unwrap();

    let reserve_status: unstake::reserve::StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.reserve, &unstake::reserve::QueryMsg::Status {})
        .unwrap();

    assert_eq!(
        reserve_status.available,
        AmountU128::new(Uint128::from(20000u128 - 824))
    );
    assert_eq!(
        reserve_status.deployed,
        AmountU128::new(Uint128::from(824u128))
    );
    assert_eq!(status.total_base, AmountU128::new(Uint128::from(10000u128)));
    assert_eq!(status.total_quote, AmountU128::zero());
}

#[test]
fn execute_unfunded_offer() {
    // Quote where we have no reserves
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("unstaker"), coins(10000u128, "base")),
        (api.addr_make("lender"), coins(100000000u128, "quote")),
    ];
    let (mut app, contracts) = setup(balances, None);

    app.execute_contract(
        api.addr_make("lender"),
        contracts.ghost.clone(),
        &kujira_ghost::receipt_vault::ExecuteMsg::Deposit(
            kujira_ghost::receipt_vault::DepositMsg { callback: None },
        ),
        &coins(100000000u128, "quote"),
    )
    .unwrap();

    let amount = AmountU128::new(Uint128::from(10000u128));

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller.clone(), &QueryMsg::Status {})
        .unwrap();

    assert_eq!(status.total_base, AmountU128::zero());
    assert_eq!(status.total_quote, AmountU128::zero());

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
    let reserve_balances = query_balances(&app, contracts.reserve.clone());
    let delegate_balances = query_balances(&app, delegate);
    let provider_balances = query_balances(&app, contracts.provider);
    let ghost_balances = query_balances(&app, contracts.ghost.clone());

    // unstaker gets their money, less the fees
    assert_eq!(unstaker_balances, coins(9501u128, "quote"));
    // remainder of reserve left on controller
    assert_eq!(reserve_balances, vec![]);
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

    let reserve_status: unstake::reserve::StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.reserve, &unstake::reserve::QueryMsg::Status {})
        .unwrap();

    assert_eq!(reserve_status.available, AmountU128::zero());
    assert_eq!(reserve_status.deployed, AmountU128::zero());
    assert_eq!(status.total_base, AmountU128::new(Uint128::from(10000u128)));
    assert_eq!(status.total_quote, AmountU128::zero());
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
    let (mut app, contracts) = setup(balances, None);
    fund_reserve(
        &mut app,
        api.addr_make("funder"),
        contracts.reserve.clone(),
        20000u128.into(),
        "quote",
    );

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

    let amount = AmountU128::new(Uint128::from(10000u128));

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

    let reserve_status: unstake::reserve::StatusResponse = app
        .wrap()
        .query_wasm_smart(
            contracts.reserve.clone(),
            &unstake::reserve::QueryMsg::Status {},
        )
        .unwrap();

    assert_eq!(
        reserve_status.available,
        AmountU128::new(Uint128::from(20000u128 - 824))
    );
    assert_eq!(
        reserve_status.deployed,
        AmountU128::new(Uint128::from(824u128))
    );
    assert_eq!(status.total_base, AmountU128::new(Uint128::from(10000u128)));
    assert_eq!(status.total_quote, AmountU128::zero());

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

    let reserve_balances = query_balances(&app, contracts.reserve.clone());
    let delegate_balances = query_balances(&app, delegate);
    let provider_balances = query_balances(&app, contracts.provider);
    let ghost_balances = query_balances(&app, contracts.ghost.clone());

    // despite the rate being as-predicted, this unstake will generate a profit.
    // this is because the unstake fee - in this case 100% APR over 2 weeks = 3.8356%
    // is charged on the amount of quote asset returned, but in order to honour this
    // unbonding, we actually only need to borrow 100% - 3.8356% of the unbonded amount.
    // Then, we also subtract the reserve allocation from the amount borrowed.

    // We can calculate our way around this when quoting, but it's a nice extra bit of revenue
    // for the protocol

    // So we'll pay 3.8356 on 9503 = 365,
    // but have an allocated fee of 3.8356 on the total value = 10737 * 3.8356 = 411
    // so we have an excess profit here of 411 - 365 = 46
    // of that profit, 25% is protocol_fee, so 11.
    assert_eq!(
        reserve_balances,
        coins(20035u128, format!("factory/{}/urcpt", contracts.ghost))
    );

    // delegate should now be empty
    assert_eq!(delegate_balances, vec![]);

    // Provider should have received the base for unbonding, plus the surplus from the original funding
    // (500,000 - (10000 * 1.07375))
    assert_eq!(
        provider_balances,
        vec![coin(10000u128, "base"), coin(489263u128, "quote")]
    );

    // And ghost should have the borrowed amount returned with interest
    // 9503 over 2 weeks at 100% = 1 / 26 = 365 + 35 extra from reserve fee + 1 for rounding
    assert_eq!(ghost_balances, coins(100020401u128, "quote"));

    let status: StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller, &QueryMsg::Status {})
        .unwrap();

    let reserve_status: unstake::reserve::StatusResponse = app
        .wrap()
        .query_wasm_smart(contracts.reserve, &unstake::reserve::QueryMsg::Status {})
        .unwrap();

    // Remainder of the profit goes onto the reserve
    assert_eq!(
        reserve_status.available,
        AmountU128::new(Uint128::from(20000u128 + 35))
    );
    assert_eq!(reserve_status.deployed, AmountU128::zero());
    assert_eq!(status.total_base, AmountU128::new(Uint128::from(10000u128)));
    // 10000 * 1.07375 for returned amount
    assert_eq!(
        status.total_quote,
        AmountU128::new(Uint128::from(10737u128))
    );
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
    let (mut app, contracts) = setup(balances, None);
    fund_reserve(
        &mut app,
        api.addr_make("funder"),
        contracts.reserve.clone(),
        20000u128.into(),
        "quote",
    );

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

    let amount = AmountU128::new(Uint128::from(10000u128));

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
    let (mut app, contracts) = setup(balances, None);
    fund_reserve(
        &mut app,
        api.addr_make("funder"),
        contracts.reserve.clone(),
        20000u128.into(),
        "quote",
    );

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

    let amount = AmountU128::new(Uint128::from(10000u128));

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
    let ghost_balances = query_balances(&app, contracts.ghost.clone());
    println!("{:?}", ghost_balances);

    app.execute_contract(
        api.addr_make("random"),
        delegate.clone(),
        &unstake::delegate::ExecuteMsg::Complete {},
        &vec![],
    )
    .unwrap();

    let reserve_balances = query_balances(&app, contracts.reserve);
    let delegate_balances = query_balances(&app, delegate);
    let provider_balances = query_balances(&app, contracts.provider);
    let ghost_balances = query_balances(&app, contracts.ghost.clone());

    // 21 days will be a total of 5.7534% interest
    // So we'll pay 5.7534 on (10326 - 823) = 547,
    // but have an allocated fee of 3.8356 on the total value = 10737 * 3.8356 = 411
    // so we have a loss  here of 411 - 547 = -136 (rounding)
    assert_eq!(
        reserve_balances,
        coins(19864u128, format!("factory/{}/urcpt", contracts.ghost))
    );

    // delegate should now be empty
    assert_eq!(delegate_balances, vec![]);

    // Provider should have received the base for unbonding, plus the surplus from the original funding
    // (500,000 - (10000 * 1.07375))
    assert_eq!(
        provider_balances,
        vec![coin(10000u128, "base"), coin(489263u128, "quote")]
    );

    // And ghost should have the borrowed amount returned with interest
    // 100000000 + 20000 + 547 - 136 = 100020411 + 1 for rounding
    // 9503 over 3 weeks at 100% = 3 / 52 = 549.
    assert_eq!(ghost_balances, coins(100020412u128, "quote"));
}

#[test]
fn reserves() {
    // Test that minting of the reserve receipt token is correct, and redeemed correctly
    let api = MockApiBech32::new("kujira");
    let balances = vec![
        (api.addr_make("funder"), coins(100000000u128, "quote")),
        (api.addr_make("unstaker"), coins(10000u128, "base")),
        (api.addr_make("lender"), coins(100000000u128, "quote")),
    ];
    let (mut app, contracts) = setup(balances, None);
    fund_reserve(
        &mut app,
        api.addr_make("funder"),
        contracts.reserve.clone(),
        20000u128.into(),
        "quote",
    );

    let rct_balance = app
        .wrap()
        .query_balance(
            api.addr_make("funder"),
            format!("factory/{}/ursv", contracts.reserve),
        )
        .unwrap();
    assert_eq!(rct_balance.amount, Uint128::from(20000u128));

    // Second deposit, scaled with no extra revenue
    fund_reserve(
        &mut app,
        api.addr_make("funder"),
        contracts.reserve.clone(),
        20000u128.into(),
        "quote",
    );

    let rct_balance = app
        .wrap()
        .query_balance(
            api.addr_make("funder"),
            format!("factory/{}/ursv", contracts.reserve),
        )
        .unwrap();
    assert_eq!(rct_balance.amount, Uint128::from(40000u128));

    // Do an unstake to increase the amount of reserves

    app.execute_contract(
        api.addr_make("lender"),
        contracts.ghost.clone(),
        &kujira_ghost::receipt_vault::ExecuteMsg::Deposit(
            kujira_ghost::receipt_vault::DepositMsg { callback: None },
        ),
        &coins(100000000u128, "quote"),
    )
    .unwrap();

    let amount = AmountU128::new(Uint128::from(10000u128));

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

    // Check we can't withdraw everything whilst reserves are being used
    app.execute_contract(
        api.addr_make("funder"),
        contracts.reserve.clone(),
        &unstake::reserve::ExecuteMsg::Withdraw { callback: None },
        &coins(40000u128, format!("factory/{}/ursv", contracts.reserve)),
    )
    .unwrap_err();

    // ...but that a smaller withdrawal also considers the consumed reserves in calculations
    let res = app
        .execute_contract(
            api.addr_make("funder"),
            contracts.reserve.clone(),
            &unstake::reserve::ExecuteMsg::Withdraw { callback: None },
            &coins(20000u128, format!("factory/{}/ursv", contracts.reserve)),
        )
        .unwrap();

    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", api.addr_make("funder").to_string()),
        ("sender", contracts.reserve.to_string()),
        ("amount", "20000quote".to_string()),
    ]));

    // 2 weeks later, ghost debt rate should have increased
    app.update_block(|x| {
        x.time = x.time.plus_days(14);
    });

    let delegates: DelegatesResponse = app
        .wrap()
        .query_wasm_smart(contracts.controller.clone(), &QueryMsg::Delegates {})
        .unwrap();
    let (delegate, _) = delegates.delegates[0].clone();

    // Make sure that the mocked LSD provider has enough tokens to return once unbonding is complete
    app.send_tokens(
        api.addr_make("funder"),
        contracts.provider.clone(),
        &coins(500000u128, "quote"),
    )
    .unwrap();

    app.execute_contract(
        api.addr_make("random"),
        delegate.clone(),
        &unstake::delegate::ExecuteMsg::Complete {},
        &vec![],
    )
    .unwrap();

    // Reserve rate is 1.0 + 35 / 20000 = 1.00175

    // Third deposit, should have slight less receipt token printed as the reserves > supply
    fund_reserve(
        &mut app,
        api.addr_make("funder"),
        contracts.reserve.clone(),
        20000u128.into(),
        "quote",
    );

    // 20000 / 1.00175 = 19965

    let rct_balance = app
        .wrap()
        .query_balance(
            api.addr_make("funder"),
            format!("factory/{}/ursv", contracts.reserve),
        )
        .unwrap();
    assert_eq!(rct_balance.amount, Uint128::from(20000u128 + 19965u128));

    // And now we withdraw some of what's left, ratio should be fractionally greater than 1
    let res = app
        .execute_contract(
            api.addr_make("funder"),
            contracts.reserve.clone(),
            &unstake::reserve::ExecuteMsg::Withdraw { callback: None },
            &coins(20000u128, format!("factory/{}/ursv", contracts.reserve)),
        )
        .unwrap();

    // 20000u128 * 1.00175 = 20035
    res.assert_event(&Event::new("transfer").add_attributes(vec![
        ("recipient", api.addr_make("funder").to_string()),
        ("sender", contracts.reserve.to_string()),
        ("amount", "20035quote".to_string()),
    ]));
}
