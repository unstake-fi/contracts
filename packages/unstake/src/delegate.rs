use crate::{adapter::Adapter, broker::Offer};
use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;

/// A delegate is instantiated for each individual Unbonding transaction.
/// We can't guarantee any specific ID to be returned from a staked token provider,
/// therefore this contract provides atomic unstaking with a known start time and
/// debt amount, such that we can calculate the debt value for only this unbonding,
/// when it completes
#[cw_serde]
pub struct InstantiateMsg {
    /// The Unstake controller address that instantiated this contract
    pub controller: Addr,

    /// The Offer created by the controller's Broker
    pub offer: Offer,

    /// The adapter for unbonding,
    pub adapter: Adapter,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Withdraws the completed unbond, and calls back to the controller to repay
    /// and handle protocol reserves
    Complete {},

    /// Callback execugted after unbonded funds have been received
    Callback {},
}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {}
