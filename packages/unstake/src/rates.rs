use cosmwasm_std::{Addr, CustomQuery, Decimal, QuerierWrapper, StdResult};

use crate::adapter::{Adapter, Unstake};

pub struct Rates {
    pub vault_debt: Decimal,
    pub vault_interest: Decimal,
    pub vault_max_interest: Decimal,
    pub provider_redemption: Decimal,
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
            vault_debt: status.debt_share_ratio,
            vault_interest: status.rate,
            vault_max_interest: Decimal::from_ratio(3u128, 1u128),
            provider_redemption,
        })
    }
}

impl Into<String> for Rates {
    fn into(self) -> String {
        format!(
            "vault_debt:{},vault_interest:{},vault_max_interest:{},provider_redemption:{}",
            self.vault_debt, self.vault_interest, self.vault_max_interest, self.provider_redemption
        )
    }
}
