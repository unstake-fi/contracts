use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::{Addr, Decimal, Uint128};
use kujira::{CallbackData, Denom};

#[cw_serde]
pub struct InstantiateMsg {
    pub owner: Addr,
    /// The base denom of the Reserve - ie the underlying bonded token
    pub base_denom: Denom,
    /// The address of the associated GHOST vault
    pub ghost_vault_addr: Addr,
}

#[cw_serde]
pub enum ExecuteMsg {
    /// Add funds to the Reserve.
    Fund { callback: Option<CallbackData> },
    /// Withdraw deposited reserve funds.
    Withdraw { callback: Option<CallbackData> },
    /// Send reserves to a controller requesting them.
    RequestReserves {
        requested_amount: Uint128,
        callback: Option<CallbackData>,
    },
    /// Accept returned reserves from a controller. Updates Reserve rates, decreasing rates if
    /// reserves are not fully returned, or increasing rates if reserves are returned alongside fees.
    ReturnReserves {
        original_amount: Uint128,
        callback: Option<CallbackData>,
    },
    /// Add the specified controller to the whitelist.
    AddController {
        controller: Addr,
        limit: Option<Uint128>,
    },
    /// Remove the specified controller from the whitelist.
    RemoveController { controller: Addr },
    /// Update the Reserve config
    UpdateConfig { owner: Option<Addr> },

    /// Migration Utility for legacy controller denoms
    MigrateLegacyReserve {},
}

#[cw_serde]
pub enum CallbackType {}

#[cw_serde]
#[derive(QueryResponses)]
pub enum QueryMsg {
    #[returns(StatusResponse)]
    Status {},
    #[returns(WhitelistResponse)]
    Whitelist {},
    #[returns(ConfigResponse)]
    Config {},
}

#[cw_serde]
pub struct ConfigResponse {
    pub owner: Addr,
    pub base_denom: Denom,
    pub rsv_denom: Denom,
    pub ghost_denom: Denom,
    pub ghost_vault_addr: Addr,
}

#[cw_serde]
pub struct WhitelistResponse {
    pub controllers: Vec<WhitelistItem>,
}

#[cw_serde]
pub struct WhitelistItem {
    pub controller: Addr,
    pub lent: Uint128,
    pub limit: Option<Uint128>,
}

#[cw_serde]
pub struct StatusResponse {
    /// The total supply of rsv tokens
    pub total_deposited: Uint128,
    /// The amount of the reserve that is currently allocated. Denominated in ghost rcpt tokens.
    pub reserves_deployed: Uint128,
    /// The amount of the reserve that is currently available. Denominated in ghost rcpt tokens.
    pub reserves_available: Uint128,
    /// The redemption ratio of rsv tokens to ghost rcpt tokens
    pub reserve_redemption_rate: Decimal,
}
