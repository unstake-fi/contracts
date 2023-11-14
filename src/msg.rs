use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Uint128;

use crate::broker::Offer;

#[cw_serde]
pub struct InstantiateMsg {}

#[cw_serde]
pub enum ExecuteMsg {
    Unstake { max_fee: Uint128 },
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
