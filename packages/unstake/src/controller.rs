use crate::{adapter::Adapter, broker::Offer, rates::Rates};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Coin, Decimal, Timestamp, Uint128};
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

    /// Update the Controller config
    UpdateConfig {
        owner: Option<Addr>,
        protocol_fee: Option<Decimal>,
        delegate_code_id: Option<u64>,
    },

    /// Update the Broker config
    UpdateBroker {
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

    #[returns(RatesResponse)]
    Rates {},

    #[returns(ConfigResponse)]
    Config {},
}

#[cw_serde]
pub struct OfferResponse {
    pub amount: Uint128,
    pub fee: Uint128,
}

#[cw_serde]
pub struct RatesResponse {
    pub debt: Decimal,
    pub interest: Decimal,
    pub max_interest: Decimal,
}

#[cw_serde]
pub struct ConfigResponse {
    pub owner: Addr,
    pub protocol_fee: Decimal,
    pub delegate_code_id: u64,
    pub vault_address: Addr,
    pub offer_denom: Denom,
    pub ask_denom: Denom,
    pub adapter: AdapterResponse,
}

#[cw_serde]
pub enum AdapterResponse {
    Contract(ContractResponse),
}

#[cw_serde]
pub struct ContractResponse {
    pub address: Addr,
    pub redemption_rate_query: Binary,
    pub unbond_start_msg: Binary,
    pub unbond_end_msg: Binary,
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

impl From<Rates> for RatesResponse {
    fn from(value: Rates) -> Self {
        Self {
            debt: value.debt,
            interest: value.interest,
            max_interest: value.max_interest,
        }
    }
}

impl From<Adapter> for AdapterResponse {
    fn from(value: Adapter) -> Self {
        match value {
            Adapter::Contract(contract) => AdapterResponse::Contract(ContractResponse {
                address: contract.address,
                redemption_rate_query: contract.redemption_rate_query,
                unbond_start_msg: contract.unbond_start_msg,
                unbond_end_msg: contract.unbond_end_msg,
            }),
        }
    }
}
