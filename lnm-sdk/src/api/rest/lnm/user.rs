use std::sync::Arc;

use async_trait::async_trait;
use hyper::Method;

use super::{
    super::{error::Result, models::user::User, repositories::UserRepository},
    base::{ApiPath, LnmRestBase},
};

pub(crate) struct LnmUserRepository {
    base: Arc<LnmRestBase>,
}

impl LnmUserRepository {
    pub fn new(base: Arc<LnmRestBase>) -> Self {
        Self { base }
    }
}

#[async_trait]
impl UserRepository for LnmUserRepository {
    async fn get_user(&self) -> Result<User> {
        self.base
            .make_request_without_params(Method::GET, ApiPath::UserGetUser, true)
            .await
    }
}

#[cfg(test)]
mod tests {
    use std::env;

    use dotenv::dotenv;

    use super::*;
    use super::{super::super::RestApiContextConfig, LnmRestBase};

    fn init_repository_from_env() -> LnmUserRepository {
        dotenv().ok();

        let domain =
            env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN environment variable must be set");
        let key = env::var("LNM_API_KEY").expect("LNM_API_KEY environment variable must be set");
        let secret =
            env::var("LNM_API_SECRET").expect("LNM_API_SECRET environment variable must be set");
        let passphrase = env::var("LNM_API_PASSPHRASE")
            .expect("LNM_API_PASSPHRASE environment variable must be set");

        let base = LnmRestBase::with_credentials(
            RestApiContextConfig::default(),
            domain,
            key,
            secret,
            passphrase,
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
