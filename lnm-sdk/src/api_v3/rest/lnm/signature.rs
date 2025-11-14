use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use reqwest::{self, Method, Url};
use sha2::Sha256;

use crate::shared::rest::{
    error::{RestApiError, Result},
    lnm::base::SignatureGenerator,
};

/// Signature generator for LNM API v3
#[derive(Clone)]
pub(in crate::api_v3) struct SignatureGeneratorV3 {
    secret: String,
}

impl SignatureGeneratorV3 {
    pub fn new(secret: String) -> Self {
        Self { secret }
    }
}

impl SignatureGenerator for SignatureGeneratorV3 {
    fn generate(
        &self,
        timestamp: DateTime<Utc>,
        method: &Method,
        url: &Url,
        body: Option<&String>,
    ) -> Result<String> {
        let timestamp_str = timestamp.timestamp_millis().to_string();

        // In v3, query params must be prefixed with '?'
        let query_with_prefix = url
            .query()
            .map(|q| {
                if q.is_empty() {
                    String::new()
                } else {
                    format!("?{q}")
                }
            })
            .unwrap_or_default();

        let params_str = match *method {
            Method::POST | Method::PUT => body.map(|v| v.as_str()).unwrap_or(""),
            Method::GET | Method::DELETE => query_with_prefix.as_str(),
            _ => "",
        };

        let prehash = format!(
            "{}{}{}{}",
            timestamp_str,
            method.as_str().to_lowercase(), // Differs from v2
            url.path(),
            params_str
        );

        let mut mac = Hmac::<Sha256>::new_from_slice(self.secret.as_bytes())
            .map_err(RestApiError::InvalidSecretHmac)?;
        mac.update(prehash.as_bytes());
        let mac = mac.finalize().into_bytes();

        let signature = BASE64.encode(mac);

        Ok(signature)
    }
}
