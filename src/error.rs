use cosmwasm_std::{OverflowError, StdError};
use thiserror::Error;

#[derive(Error, Debug)]
pub enum ContractError {
    #[error("{0}")]
    Std(#[from] StdError),

    #[error("Unauthorized")]
    Unauthorized {},

    #[error("RateOverflow {0}")]
    RateOverflow(#[from] OverflowError),

    #[error("InsufficentReserves")]
    InsufficentReserves {},
}
