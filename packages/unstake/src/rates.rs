use cosmwasm_std::{Addr, CustomQuery, Decimal, QuerierWrapper, StdResult};

pub struct Rates {
    pub debt: Decimal,
    pub interest: Decimal,
    pub max_interest: Decimal,
}

impl Rates {
    pub fn load<C: CustomQuery>(query: QuerierWrapper<C>, vault: &Addr) -> StdResult<Self> {
        let status: kujira_ghost::receipt_vault::StatusResponse = query.query_wasm_smart(
            vault.to_string(),
            &kujira_ghost::receipt_vault::QueryMsg::Status {},
        )?;

        // TODO: Publish & use new interest rate params
        // let rates: kujira_ghost::receipt_vault::InterestParamsResponse = query.query_wasm_smart(
        //     self.vault.to_string(),
        //     &kujira_ghost::receipt_vault::QueryMsg::InterestParams {},
        // )?;

        Ok(Self {
            debt: status.debt_share_ratio,
            interest: status.rate,
            max_interest: Decimal::from_ratio(3u128, 1u128),
        })
    }
}
