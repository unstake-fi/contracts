use cosmwasm_schema::{cw_serde, QueryResponses};
use cosmwasm_std::Addr;
use kujira::CallbackData;
use monetary::{AmountU128, Denom, Rate};

use crate::denoms::{Base, Rcpt, Rsv};

#[cw_serde]
pub struct InstantiateMsg {
    pub owner: Addr,
    /// The base denom of the Reserve - ie the underlying bonded token
    pub base_denom: Denom<Base>,
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
        requested_amount: AmountU128<Base>,
        callback: Option<CallbackData>,
    },
    /// Accept returned reserves from a controller. Updates Reserve rates, decreasing rates if
    /// reserves are not fully returned, or increasing rates if reserves are returned alongside fees.
    ReturnReserves {
        original_amount: AmountU128<Base>,
        callback: Option<CallbackData>,
    },
    /// Add the specified controller to the whitelist.
    AddController {
        controller: Addr,
        limit: Option<AmountU128<Base>>,
    },
    /// Remove the specified controller from the whitelist.
    RemoveController { controller: Addr },
    /// Update the Reserve config
    UpdateConfig { owner: Option<Addr> },

    /// Migration Utility for legacy controller denoms
    MigrateLegacyReserve { reserves_deployed: AmountU128<Base> },
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
    pub base_denom: Denom<Base>,
    pub rsv_denom: Denom<Rsv>,
    pub ghost_denom: Denom<Rcpt>,
    pub ghost_vault_addr: Addr,
}

#[cw_serde]
pub struct WhitelistResponse {
    pub controllers: Vec<WhitelistItem>,
}

#[cw_serde]
pub struct WhitelistItem {
    pub controller: Addr,
    pub lent: AmountU128<Base>,
    pub limit: Option<AmountU128<Base>>,
}

#[cw_serde]
pub struct StatusResponse {
    /// The total amount deposited in the reserve, denominated in the base token.
    pub total: AmountU128<Base>,
    /// The amount of the reserve that is currently allocated. Denominated in the base token.
    pub deployed: AmountU128<Base>,
    /// The amount of the reserve that is currently available. Denominated in ghost rcpt tokens.
    pub available: AmountU128<Rcpt>,
    /// The redemption ratio of rsv tokens to ghost rcpt tokens
    pub reserve_redemption_rate: Rate<Base, Rsv>,
}
