use std::{env, time::Instant};

use dotenv::dotenv;

use crate::shared::config::RestClientConfig;

use super::*;

fn init_repository_from_env() -> LnmUtilitiesRepository {
    dotenv().ok();

    let domain =
        env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN environment variable must be set");

    let base =
        LnmRestBase::new(RestClientConfig::default(), domain).expect("Can create `LnmApiBase`");

    LnmUtilitiesRepository::new(base)
}

#[tokio::test]
#[ignore]
async fn test_api() {
    let repo = init_repository_from_env();

    macro_rules! time_test {
        ($test_name: expr, $test_block: expr) => {{
            println!("\nStarting test: {}", $test_name);
            let start = Instant::now();
            let result = $test_block;
            let elapsed = start.elapsed();
            println!("Test '{}' took: {:?}", $test_name, elapsed);
            result
        }};
    }

    // Start tests

    time_test!("test_ping", repo.ping().await).unwrap();

    let _ = time_test!("test_time", repo.time().await).unwrap();
}
