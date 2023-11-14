use crate::broker::Offer;
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Uint128};
use kujira::Denom;

#[cw_serde]
pub struct InstantiateMsg {
    pub owner: Addr,
    pub delegate_code_id: u8,
    pub vault_address: Addr,
    /// The ask denom of the Broker - ie the LST/receipt token
    pub ask_denom: Denom,

    /// The offer denom of the Broker - ie the underlying bonded token
    pub offer_denom: Denom,
}

#[cw_serde]
pub enum ExecuteMsg {
    Unstake {
        max_fee: Uint128,
    },
    /// Called by a delegate contract when the unbonding process is complete.
    /// Returns the unbonded tokens, the debt tokens for ghost, and the corresponding offer
    Complete {
        offer: Offer,
    },
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(OfferResponse)]
    Offer { amount: Uint128 },
}

#[cw_serde]
pub struct OfferResponse {
    amount: Uint128,
    fee: Uint128,
}

impl From<Offer> for OfferResponse {
    fn from(value: Offer) -> Self {
        Self {
            amount: value.amount,
            fee: value.fee,
        }
    }
}
