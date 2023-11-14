use cosmwasm_std::{OverflowError, StdError};
use cw_utils::PaymentError;
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("RateOverflow {0}")]
    RateOverflow(#[from] OverflowError),

    #[error("{0}")]
    Payment(#[from] PaymentError),

    #[error("InsufficentReserves")]
    InsufficentReserves {},

    #[error("MaxFeeExceeded")]
    MaxFeeExceeded {},
}
