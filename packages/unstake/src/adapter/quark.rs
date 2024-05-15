use cosmwasm_schema::cw_serde;
use cosmwasm_std::{
    wasm_execute, Addr, Coin, CosmosMsg, CustomQuery, Decimal, QuerierWrapper, StdResult,
};

use super::interface::Unstake;
#[cw_serde]
pub struct Quark {
    liq: Addr,
    hub: Addr,
}

impl Quark {
    pub fn new(liq: Addr, hub: Addr) -> Self {
        Self { liq, hub }
    }
}

impl Unstake for Quark {
    fn redemption_rate<T: CustomQuery>(&self, querier: QuerierWrapper<T>) -> StdResult<Decimal> {
        let state: liquifier::StateResponse =
            querier.query_wasm_smart(&self.liq, &liquifier::QueryMsg::State {})?;
        Ok(state.rate)
    }

    fn unbond_start<T>(&self, funds: Coin) -> CosmosMsg<T> {
        wasm_execute(
            &self.liq,
            &liquifier::ExecuteMsg::Unwrap(liquifier::UnwrapMsg {
                target: Some(liquifier::UnwrapTarget::Underlying),
                callback: None,
            }),
            vec![funds],
        )
        .unwrap()
        .into()
    }

    fn unbond_end<T>(&self) -> CosmosMsg<T> {
        wasm_execute(
            &self.hub,
            &hub::ExecuteMsg::Withdraw(hub::WithdrawMsg {
                recipient: None,
                callback: None,
            }),
            vec![],
        )
        .unwrap()
        .into()
    }
}

mod liquifier {
    use cosmwasm_schema::{cw_serde, QueryResponses};
    use cosmwasm_std::{Decimal, Uint128};
    use kujira::CallbackData;
    #[cw_serde]
    #[derive(QueryResponses)]
    pub enum QueryMsg {
        #[returns(StateResponse)]
        State {},
    }

    #[cw_serde]
    pub struct StateResponse {
        pub supply: Uint128,
        pub rate: Decimal,
    }

    #[cw_serde]
    pub enum ExecuteMsg {
        /// Unwrap LSD tokens into either hub, unifier, or underlying tokens.
        /// If unwrapping into underlying, this will begin the unstake process on the hub contract.
        /// Unstake progress is tracked by the hub contract.
        Unwrap(UnwrapMsg),
    }

    #[cw_serde]
    pub enum UnwrapTarget {
        Hub,
        Underlying,
    }
    #[cw_serde]
    pub struct UnwrapMsg {
        pub target: Option<UnwrapTarget>,
        pub callback: Option<CallbackData>,
    }
}

mod hub {
    use cosmwasm_schema::cw_serde;
    use cosmwasm_std::Addr;
    use kujira::CallbackData;

    #[cw_serde]
    pub enum ExecuteMsg {
        /// Withdraw unbonded tokens after the unbond period.
        Withdraw(WithdrawMsg),
    }

    #[cw_serde]
    pub struct WithdrawMsg {
        pub recipient: Option<Addr>,
        pub callback: Option<CallbackData>,
    }
}
