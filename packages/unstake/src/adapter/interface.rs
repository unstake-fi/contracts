use cosmwasm_schema::cw_serde;
use cosmwasm_std::{Coin, CosmosMsg, CustomQuery, Decimal, QuerierWrapper, StdResult};

use super::{eris::Eris, gravedigger::Gravedigger,quark::Quark};

pub trait Unstake {
    fn redemption_rate<T: CustomQuery>(&self, querier: QuerierWrapper<T>) -> StdResult<Decimal>;
    fn unbond_start<T>(&self, funds: Coin) -> CosmosMsg<T>;
    fn unbond_end<T>(&self) -> CosmosMsg<T>;
}

impl Unstake for Adapter {
    fn redemption_rate<T: CustomQuery>(&self, querier: QuerierWrapper<T>) -> StdResult<Decimal> {
        match self {
            Adapter::Eris(eris) => eris.redemption_rate(querier),
            Adapter::Gravedigger(gravedigger) => gravedigger.redemption_rate(querier),
            Adapter::Quark(quark) => quark.redemption_rate(querier),
        }
    }

    fn unbond_start<T>(&self, funds: Coin) -> CosmosMsg<T> {
        match self {
            Adapter::Eris(eris) => eris.unbond_start(funds),
            Adapter::Gravedigger(gravedigger) => gravedigger.unbond_start(funds),
            Adapter::Quark(quark) => quark.unbond_start(funds),
        }
    }

    fn unbond_end<T>(&self) -> CosmosMsg<T> {
        match self {
            Adapter::Eris(eris) => eris.unbond_end(),
            Adapter::Gravedigger(gravedigger) => gravedigger.unbond_end(),
            Adapter::Quark(quark) => quark.unbond_end(),
        }
    }
}

#[cw_serde]
pub enum Adapter {
    Eris(Eris),
    Gravedigger(Gravedigger),
    Quark(Quark),
}
