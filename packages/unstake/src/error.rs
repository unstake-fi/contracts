use cosmwasm_std::{Instantiate2AddressError, OverflowError, StdError, Uint128};
use cw_utils::PaymentError;
use monetary::MonetaryError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Instantiate2Address {0}")]
    Instantiate2Address(#[from] Instantiate2AddressError),

    #[error("RateOverflow {0}")]
    RateOverflow(#[from] OverflowError),

    #[error("{0}")]
    Payment(#[from] PaymentError),

    #[error("InvalidAmount")]
    Monetary(#[from] MonetaryError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("InsufficentReserves")]
    InsufficentReserves {},

    #[error("ControllerLimitExceeded")]
    ControllerLimitExceeded {},

    #[error("InsufficentFunds")]
    InsufficentFunds {},

    #[error("MaxFeeExceeded")]
    MaxFeeExceeded {},

    #[error("Insolvent {debt_remaining} remaining")]
    Insolvent { debt_remaining: Uint128 },
}
