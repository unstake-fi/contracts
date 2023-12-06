use crate::{
    adapter::Adapter,
    broker::{Offer, Status},
    rates::Rates,
};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Binary, Coin, Decimal, Timestamp, Uint128};
use kujira::{CallbackData, CallbackMsg, Denom};

#[cw_serde]
pub struct InstantiateMsg {
    pub owner: Addr,
    pub protocol_fee: Decimal,
    pub protocol_fee_address: Addr,
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
        callback: Option<CallbackData>,
    },

    /// Called after the GHOST withdrawal has been made.
    /// At this point, the only funds on the contract will be the received debt tokens from GHOST,
    /// and the received Ask tokens from the user
    Callback(CallbackMsg),

    /// Called by a delegate contract when the unbonding process is complete.
    /// Returns the unbonded tokens, the debt tokens for ghost, and the corresponding offer
    Complete { offer: Offer },

    /// Adds funds to the reserve
    Fund {},

    /// Update the Controller config
    UpdateConfig {
        owner: Option<Addr>,
        protocol_fee: Option<Decimal>,
        protocol_fee_address: Option<Addr>,
        delegate_code_id: Option<u64>,
    },

    /// Update the Broker config
    UpdateBroker {
        min_rate: Option<Decimal>,
        duration: Option<u64>,
    },
}

#[cw_serde]
pub enum CallbackType {
    Unstake { offer: Offer, unbond_amount: Coin },
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

    #[returns(StatusResponse)]
    Status {},

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
    pub vault_debt: Decimal,
    pub vault_interest: Decimal,
    pub vault_max_interest: Decimal,
    pub provider_redemption: Decimal,
}

#[cw_serde]
pub struct StatusResponse {
    /// The total amount of base asset that has been requested for unbonding
    pub total_base: Uint128,
    /// The total amount of quote asset that has been returned from unbonding
    pub total_quote: Uint128,
    /// The amount of reserve currently available for new Unstakes
    pub reserve_available: Uint128,
    /// The amount of reserve currently deployed in in-flight Unstakes
    pub reserve_deployed: Uint128,
}

#[cw_serde]
pub struct ConfigResponse {
    pub owner: Addr,
    pub protocol_fee: Decimal,
    pub protocol_fee_address: Addr,
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
            amount: value.offer_amount,
            fee: value.fee,
        }
    }
}

impl From<Rates> for RatesResponse {
    fn from(value: Rates) -> Self {
        Self {
            vault_debt: value.vault_debt,
            vault_interest: value.vault_interest,
            vault_max_interest: value.vault_max_interest,
            provider_redemption: value.provider_redemption,
        }
    }
}

impl From<Adapter> for AdapterResponse {
    fn from(value: Adapter) -> Self {
        match value {
            Adapter::Contract(contract) => AdapterResponse::Contract(ContractResponse {
                address: contract.address,
                unbond_start_msg: contract.unbond_start_msg,
                unbond_end_msg: contract.unbond_end_msg,
            }),
        }
    }
}

impl From<Status> for StatusResponse {
    fn from(value: Status) -> Self {
        Self {
            total_base: value.total_base,
            total_quote: value.total_quote,
            reserve_available: value.reserve_available,
            reserve_deployed: value.reserve_deployed,
        }
    }
}
