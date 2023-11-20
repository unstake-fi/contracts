use crate::{adapter::Adapter, broker::Offer};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Coin, Decimal, Timestamp, Uint128};
use kujira::Denom;

#[cw_serde]
pub struct InstantiateMsg {
    pub owner: Addr,
    pub protocol_fee: Decimal,
    pub delegate_code_id: u64,
    pub vault_address: Addr,

    /// The ask denom of the Broker - ie the LST/receipt token
    pub ask_denom: Denom,

    /// The offer denom of the Broker - ie the underlying bonded token
    pub offer_denom: Denom,

    /// The amount of time in seconds that an unbonding takes
    pub unbonding_duration: u64,

    /// The minimum offer rate set on the broker
    pub min_rate: Decimal,

    /// The adapter for the unbonding process
    pub adapter: Adapter,
}

#[cw_serde]
pub enum ExecuteMsg {
    Unstake {
        max_fee: Uint128,
    },

    /// Called after the GHOST withdrawal has been made.
    /// At this point, the only funds on the contract will be the received debt tokens from GHOST,
    /// and the received Ask tokens from the user
    UnstakeCallback {
        offer: Offer,
        unbond_amount: Coin,
    },

    /// Called by a delegate contract when the unbonding process is complete.
    /// Returns the unbonded tokens, the debt tokens for ghost, and the corresponding offer
    Complete {
        offer: Offer,
    },

    /// Adds funds to the reserve
    Fund {},

    /// Update the Broker config
    UpdateBroker {
        vault: Option<Addr>,
        min_rate: Option<Decimal>,
        duration: Option<u64>,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(OfferResponse)]
    Offer { amount: Uint128 },

    #[returns(DelegatesResponse)]
    Delegates {},
}

#[cw_serde]
pub struct OfferResponse {
    pub amount: Uint128,
    pub fee: Uint128,
}

#[cw_serde]
pub struct DelegatesResponse {
    pub delegates: Vec<(Addr, Timestamp)>,
}

impl From<Offer> for OfferResponse {
    fn from(value: Offer) -> Self {
        Self {
            amount: value.amount,
            fee: value.fee,
        }
    }
}
