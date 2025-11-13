use base64::{Engine, engine::general_purpose::STANDARD as BASE64};
use chrono::{DateTime, Utc};
use hmac::{Hmac, Mac};
use reqwest::{self, Method};
use sha2::Sha256;

use crate::shared::rest::{
    error::{RestApiError, Result},
    lnm::base::{RestPath, SignatureGenerator},
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
    fn generate<P: RestPath>(
        &self,
        timestamp: DateTime<Utc>,
        method: &Method,
        path: P,
        params_str: Option<&String>,
    ) -> Result<String> {
        let timestamp_str = timestamp.timestamp_millis().to_string();
        let params_str = params_str.map(|v| v.as_ref()).unwrap_or("");

        let prehash = format!(
            "{}{}{}{}",
            timestamp_str,
            method.as_str().to_lowercase(), // Differs from v2
            path.to_path_string(),
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
