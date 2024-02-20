use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Addr, Coin, CosmosMsg, CustomQuery, Decimal, QuerierWrapper, StdResult};

use super::{eris::Eris, interface::Unstake};
#[cw_serde]
pub struct Quark(Eris);

// The address provided is the bow-compat contract by Quark, so it uses the same interfaces as Eris
impl Unstake for Quark {
    fn redemption_rate<T: CustomQuery>(&self, querier: QuerierWrapper<T>) -> StdResult<Decimal> {
        self.0.redemption_rate(querier)
    }

    fn unbond_start<T>(&self, funds: Coin) -> CosmosMsg<T> {
        self.0.unbond_start(funds)
    }

    fn unbond_end<T>(&self) -> CosmosMsg<T> {
        self.0.unbond_end()
    }
}

impl From<Addr> for Quark {
    fn from(value: Addr) -> Self {
        Self(Eris::from(value))
    }
}
