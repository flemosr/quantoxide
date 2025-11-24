use std::{num::NonZeroU64, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use reqwest::{self, Method};

use crate::shared::rest::{error::Result, lnm::base::LnmRestBase};

use super::{
    super::{
        models::{
            funding::FundingSettlement,
            futures_data::{OhlcCandle, OhlcRange},
            page::Page,
            ticker::Ticker,
        },
        repositories::FuturesDataRepository,
    },
    path::RestPathV3,
    signature::SignatureGeneratorV3,
};

pub(in crate::api_v3) struct LnmFuturesDataRepository {
    base: Arc<LnmRestBase<SignatureGeneratorV3>>,
}

impl LnmFuturesDataRepository {
    pub fn new(base: Arc<LnmRestBase<SignatureGeneratorV3>>) -> Self {
        Self { base }
    }
}

impl crate::sealed::Sealed for LnmFuturesDataRepository {}

#[async_trait]
impl FuturesDataRepository for LnmFuturesDataRepository {
    async fn get_funding_settlements(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<Page<FundingSettlement>> {
        let mut query_params = Vec::new();

        if let Some(from) = from {
            query_params.push(("from", from.to_rfc3339_opts(SecondsFormat::Millis, true)));
        }
        if let Some(to) = to {
            query_params.push(("to", to.to_rfc3339_opts(SecondsFormat::Millis, true)));
        }
        if let Some(limit) = limit {
            query_params.push(("limit", limit.to_string()));
        }
        if let Some(cursor) = cursor {
            query_params.push((
                "cursor",
                cursor.to_rfc3339_opts(SecondsFormat::Millis, true),
            ));
        }

        self.base
            .make_request_with_query_params(
                Method::GET,
                RestPathV3::FuturesDataFundingSettlements,
                query_params,
                false,
            )
            .await
    }

    async fn get_ticker(&self) -> Result<Ticker> {
        self.base
            .make_request_without_params(Method::GET, RestPathV3::FuturesDataTicker, false)
            .await
    }

    async fn get_candles(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        range: Option<OhlcRange>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<Page<OhlcCandle>> {
        let mut query_params = Vec::new();

        if let Some(from) = from {
            query_params.push(("from", from.to_rfc3339_opts(SecondsFormat::Millis, true)));
        }
        if let Some(to) = to {
            query_params.push(("to", to.to_rfc3339_opts(SecondsFormat::Millis, true)));
        }
        if let Some(limit) = limit {
            query_params.push(("limit", limit.to_string()));
        }
        if let Some(range) = range {
            query_params.push(("range", range.to_string()));
        }
        if let Some(cursor) = cursor {
            query_params.push((
                "cursor",
                cursor.to_rfc3339_opts(SecondsFormat::Millis, true),
            ));
        }

        self.base
            .make_request_with_query_params(
                Method::GET,
                RestPathV3::FuturesDataGetCandles,
                query_params,
                false,
            )
            .await
    }
}

#[cfg(test)]
mod tests;
