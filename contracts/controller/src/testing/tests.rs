use std::str::FromStr;

use cosmwasm_std::{to_json_binary, Addr, Decimal, Uint128};
use cw_multi_test::{ContractWrapper, Executor};
use kujira::{Denom, HumanPrice};
use kujira_ghost::common::OracleType;
use kujira_rs_testing::mock::mock_app;
use unstake::adapter::Contract;

struct Contracts {
    pub ghost: Addr,
    pub provider: Addr,
    pub controller: Addr,
}

fn setup() -> Contracts {
    let mut app = mock_app(vec![]);
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
                ask_denom: Denom::from("quote"),
                offer_denom: Denom::from("base"),
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
    Contracts {
        ghost: vault_address,
        provider: provider_address,
        controller: controller_address,
    }
}

#[test]
fn instantiate() {
    setup();
}

#[test]
fn quote_initial() {
    // Check that when the contract is new, and there is no reserve fund, the quoted rate uses the max rate from the vault
    let contracts = setup();
}
