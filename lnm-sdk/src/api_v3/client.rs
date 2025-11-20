use std::sync::Arc;

use crate::shared::{config::ApiClientConfig, rest::error::Result as RestResult};

use super::rest::RestClient;

/// Client for interacting with the [LNM's v3 API] via REST and WebSocket.
///
/// `ApiClient` provides a interface for making REST API calls.
///
/// [LNM's v3 API]: https://api.lnmarkets.com/v3
pub struct ApiClient {
    pub rest: RestClient,
}

impl ApiClient {
    fn new_inner(rest: RestClient) -> Arc<Self> {
        Arc::new(Self { rest })
    }

    /// Creates a new unauthenticated API client.
    ///
    /// For authenticated endpoints, use [`ApiClient::with_credentials`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::api_v3::{ApiClient, ApiClientConfig};
    ///
    /// let domain = env::var("LNM_API_DOMAIN").unwrap();
    ///
    /// let api = ApiClient::new(ApiClientConfig::default(), domain)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn new(config: ApiClientConfig, domain: impl ToString) -> RestResult<Arc<Self>> {
        let domain = domain.to_string();

        let rest = RestClient::new(&config, domain)?;

        Ok(Self::new_inner(rest))
    }

    /// Creates a new authenticated API client with credentials.
    ///
    /// If not accessing authenticated endpoints, consider using [`ApiClient::new`].
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example() -> Result<(), Box<dyn std::error::Error>> {
    /// use std::env;
    /// use lnm_sdk::api_v3::{ApiClient, ApiClientConfig};
    ///
    /// let domain = env::var("LNM_API_DOMAIN").unwrap();
    /// let key = env::var("LNM_API_KEY").unwrap();
    /// let secret = env::var("LNM_API_SECRET").unwrap();
    /// let pphrase = env::var("LNM_API_PASSPHRASE").unwrap();
    ///
    /// let config = ApiClientConfig::default();
    /// let api = ApiClient::with_credentials(config, domain, key, secret, pphrase)?;
    /// # Ok(())
    /// # }
    /// ```
    pub fn with_credentials(
        config: ApiClientConfig,
        domain: impl ToString,
        key: impl ToString,
        secret: impl ToString,
        passphrase: impl ToString,
    ) -> RestResult<Arc<Self>> {
        let domain = domain.to_string();
        let key = key.to_string();
        let secret = secret.to_string();
        let passphrase = passphrase.to_string();

        let rest = RestClient::with_credentials(&config, domain, key, secret, passphrase)?;

        Ok(Self::new_inner(rest))
    }
}
