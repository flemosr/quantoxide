use std::sync::Arc;

use async_trait::async_trait;
use hyper::Method;

use crate::shared::rest::{error::Result, lnm::base::LnmRestBase};

use super::{
    super::{models::user::User, repositories::UserRepository},
    path::RestPathV2,
    signature::SignatureGeneratorV2,
};

pub(in crate::api_v2) struct LnmUserRepository {
    base: Arc<LnmRestBase<SignatureGeneratorV2>>,
}

impl LnmUserRepository {
    pub fn new(base: Arc<LnmRestBase<SignatureGeneratorV2>>) -> Self {
        Self { base }
    }
}

impl crate::sealed::Sealed for LnmUserRepository {}

#[async_trait]
impl UserRepository for LnmUserRepository {
    async fn get_user(&self) -> Result<User> {
        self.base
            .make_request_without_params(Method::GET, RestPathV2::UserGetUser, true)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use dotenv::dotenv;

    use super::*;
    use super::{super::super::RestClientConfig, LnmRestBase};

    fn init_repository_from_env() -> LnmUserRepository {
        dotenv().ok();

        let domain =
            env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN environment variable must be set");
        let key = env::var("LNM_API_V2_KEY").expect("LNM_API_KEY environment variable must be set");
        let secret =
            env::var("LNM_API_V2_SECRET").expect("LNM_API_SECRET environment variable must be set");
        let passphrase = env::var("LNM_API_V2_PASSPHRASE")
            .expect("LNM_API_V2_PASSPHRASE environment variable must be set");

        let base = LnmRestBase::with_credentials(
            RestClientConfig::default(),
            domain,
            key,
            passphrase,
            SignatureGeneratorV2::new(secret),
        )
        .expect("must create `LnmApiBase`");

        LnmUserRepository::new(base)
    }

    async fn test_get_user(repo: &LnmUserRepository) -> User {
        repo.get_user().await.expect("must get user")
    }

    #[tokio::test]
    async fn test_api() {
        let repo = init_repository_from_env();

        let _ = test_get_user(&repo).await;
    }
}
