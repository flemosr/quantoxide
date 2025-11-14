use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use reqwest::{self, Method, Url};
use sha2::Sha256;

use crate::shared::rest::{
    error::{RestApiError, Result},
    lnm::base::SignatureGenerator,
};

/// Signature generator for LNM API v2
#[derive(Clone)]
pub(in crate::api_v2) struct SignatureGeneratorV2 {
    secret: String,
}

impl SignatureGeneratorV2 {
    pub fn new(secret: String) -> Self {
        Self { secret }
    }
}

impl SignatureGenerator for SignatureGeneratorV2 {
    fn generate(
        &self,
        timestamp: DateTime<Utc>,
        method: &Method,
        url: &Url,
        body: Option<&String>,
    ) -> Result<String> {
        let timestamp_str = timestamp.timestamp_millis().to_string();

        let params_str = match *method {
            Method::POST | Method::PUT => body.map(|v| v.as_ref()).unwrap_or(""),
            Method::GET | Method::DELETE => url.query().unwrap_or(""), // Differs from v3, no '?'
            _ => "",
        };

        let prehash = format!(
            "{}{}{}{}",
            timestamp_str,
            method.as_str(), // Differs from v3
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
