use cosmwasm_std::{Addr, CustomQuery, Decimal, QuerierWrapper, StdResult};
use monetary::Rate;

use crate::{
    adapter::{Adapter, Unstake},
    denoms::{Ask, Base, Debt, Rcpt},
};

pub struct Rates {
    pub vault_debt: Rate<Base, Debt>,
    pub vault_deposit: Rate<Base, Rcpt>,
    pub vault_interest: Decimal,
    pub vault_max_interest: Decimal,
    pub provider_redemption: Rate<Base, Ask>,
}

impl Rates {
    pub fn load<C: CustomQuery>(
        query: QuerierWrapper<C>,
        adapter: &Adapter,
        vault: &Addr,
    ) -> StdResult<Self> {
        let status: kujira_ghost::receipt_vault::StatusResponse = query.query_wasm_smart(
            vault.to_string(),
            &kujira_ghost::receipt_vault::QueryMsg::Status {},
        )?;

        let provider_redemption = adapter.redemption_rate(query)?;

        // TODO: Publish & use new interest rate params
        // let rates: kujira_ghost::receipt_vault::InterestParamsResponse = query.query_wasm_smart(
        //     self.vault.to_string(),
        //     &kujira_ghost::receipt_vault::QueryMsg::InterestParams {},
        // )?;

        Ok(Self {
            vault_debt: Rate::new(status.debt_share_ratio).unwrap(),
            vault_deposit: Rate::new(status.deposit_redemption_ratio).unwrap(),
            vault_interest: status.rate,
            vault_max_interest: Decimal::from_ratio(3u128, 1u128),
            provider_redemption: Rate::new(provider_redemption).unwrap(),
        })
    }
}

impl From<Rates> for String {
    fn from(value: Rates) -> Self {
        format!(
            "vault_debt:{},vault_interest:{},vault_max_interest:{},provider_redemption:{}",
            value.vault_debt,
            value.vault_interest,
            value.vault_max_interest,
            value.provider_redemption
        )
    }
}
