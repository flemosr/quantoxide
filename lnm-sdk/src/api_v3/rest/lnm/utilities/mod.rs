use std::sync::Arc;

use async_trait::async_trait;
use chrono::{DateTime, Utc};
use reqwest::{self, Method};
use serde::Deserialize;

use crate::shared::rest::{error::Result, lnm::base::LnmRestBase};

use super::{
    super::{error::RestApiV3Error, repositories::UtilitiesRepository},
    path::RestPathV3,
    signature::SignatureGeneratorV3,
};

pub(in crate::api_v3) struct LnmUtilitiesRepository {
    base: Arc<LnmRestBase<SignatureGeneratorV3>>,
}

impl LnmUtilitiesRepository {
    pub fn new(base: Arc<LnmRestBase<SignatureGeneratorV3>>) -> Self {
        Self { base }
    }
}

impl crate::sealed::Sealed for LnmUtilitiesRepository {}

#[async_trait]
impl UtilitiesRepository for LnmUtilitiesRepository {
    async fn ping(&self) -> Result<()> {
        let res = self
            .base
            .make_get_request_plain_text(RestPathV3::UtilitiesPing)
            .await?;

        if res.as_str() == "pong" {
            Ok(())
        } else {
            return Err(RestApiV3Error::UnexpectedPingResponse(res).into());
        }
    }

    async fn time(&self) -> Result<DateTime<Utc>> {
        #[derive(Deserialize)]
        struct UtilitiesTimeResponse {
            time: DateTime<Utc>,
        }

        let res: UtilitiesTimeResponse = self
            .base
            .make_request_without_params(Method::GET, RestPathV3::UtilitiesTime, false)
            .await?;

        Ok(res.time)
    }
}

#[cfg(test)]
mod tests;
