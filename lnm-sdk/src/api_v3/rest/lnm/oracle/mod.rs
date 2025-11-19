use std::{num::NonZeroU64, sync::Arc};

use async_trait::async_trait;
use chrono::{DateTime, SecondsFormat, Utc};
use hyper::Method;

use crate::shared::rest::{error::Result, lnm::base::LnmRestBase};

use super::{
    super::{
        models::oracle::{Index, LastPrice},
        repositories::OracleRepository,
    },
    path::RestPathV3,
    signature::SignatureGeneratorV3,
};

pub(in crate::api_v3) struct LnmOracleRepository {
    base: Arc<LnmRestBase<SignatureGeneratorV3>>,
}

impl LnmOracleRepository {
    pub fn new(base: Arc<LnmRestBase<SignatureGeneratorV3>>) -> Self {
        Self { base }
    }
}

impl crate::sealed::Sealed for LnmOracleRepository {}

#[async_trait]
impl OracleRepository for LnmOracleRepository {
    async fn get_index(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<Vec<Index>> {
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
                RestPathV3::OracleIndex,
                query_params,
                false,
            )
            .await
    }

    async fn get_last_price(
        &self,
        from: Option<DateTime<Utc>>,
        to: Option<DateTime<Utc>>,
        limit: Option<NonZeroU64>,
        cursor: Option<DateTime<Utc>>,
    ) -> Result<Vec<LastPrice>> {
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
                RestPathV3::OracleLastPrice,
                query_params,
                false,
            )
            .await
    }
}

#[cfg(test)]
mod tests;
