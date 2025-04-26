use async_trait::async_trait;
use hyper::Method;
use std::sync::Arc;

use super::{
    super::{error::Result, models::User, repositories::UserRepository},
    base::{ApiPath, LnmApiBase},
};

pub struct LnmUserRepository {
    base: Arc<LnmApiBase>,
}

impl LnmUserRepository {
    pub fn new(base: Arc<LnmApiBase>) -> Self {
        Self { base }
    }
}

#[async_trait]
impl UserRepository for LnmUserRepository {
    async fn get_user(&self) -> Result<User> {
        let user: User = self
            .base
            .make_request_without_params(Method::GET, &ApiPath::UserGetUser, true)
            .await?;

        Ok(user)
    }
}

#[cfg(test)]
mod tests {
    use dotenv::dotenv;
    use std::env;

    use crate::api::rest::lnm::base::LnmApiBase;

    use super::*;

    fn init_repository_from_env() -> LnmUserRepository {
        dotenv().ok();

        let domain =
            env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN environment variable must be set");
        let key = env::var("LNM_API_KEY").expect("LNM_API_KEY environment variable must be set");
        let secret =
            env::var("LNM_API_SECRET").expect("LNM_API_SECRET environment variable must be set");
        let passphrase = env::var("LNM_API_PASSPHRASE")
            .expect("LNM_API_PASSPHRASE environment variable must be set");

        let base =
            LnmApiBase::new(domain, key, secret, passphrase).expect("Can create `LnmApiBase`");

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
