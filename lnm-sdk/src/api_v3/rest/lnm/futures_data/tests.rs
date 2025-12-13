use std::{env, time::Instant};

use dotenv::dotenv;

use crate::shared::config::RestClientConfig;

use super::*;

fn init_repository_from_env() -> LnmFuturesDataRepository {
    dotenv().ok();

    let domain =
        env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN environment variable must be set");
    let key = env::var("LNM_API_V3_KEY").expect("LNM_API_V3_KEY environment variable must be set");
    let secret =
        env::var("LNM_API_V3_SECRET").expect("LNM_API_V3_SECRET environment variable must be set");
    let passphrase = env::var("LNM_API_V3_PASSPHRASE")
        .expect("LNM_API_V3_PASSPHRASE environment variable must be set");

    let base = LnmRestBase::with_credentials(
        RestClientConfig::default(),
        domain,
        key,
        passphrase,
        SignatureGeneratorV3::new(secret),
    )
    .expect("must create `LnmApiBase`");

    LnmFuturesDataRepository::new(base)
}

async fn test_get_funding_settlements(repo: &LnmFuturesDataRepository) {
    let _ = repo
        .get_funding_settlements(None, None, None, None)
        .await
        .expect("must get funding settlements");
}

async fn test_ticker(repo: &LnmFuturesDataRepository) {
    let ticker = repo.get_ticker().await.expect("must get ticker");

    assert!(!ticker.prices().is_empty());
}

async fn test_get_max_candles(repo: &LnmFuturesDataRepository) {
    let limit = 1000.try_into().unwrap();
    let _ = repo
        .get_candles(None, None, Some(limit), Some(OhlcRange::OneMinute), None)
        .await
        .expect("must get candles");
}

async fn test_get_last_candle(repo: &LnmFuturesDataRepository) {
    let limit = 1.try_into().unwrap();
    let _ = repo
        .get_candles(None, None, Some(limit), Some(OhlcRange::OneMinute), None)
        .await
        .expect("must get candles");
}

#[tokio::test]
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

    time_test!("test_ticker", test_ticker(&repo).await);

    time_test!(
        "test_get_funding_settlements",
        test_get_funding_settlements(&repo).await
    );

    time_test!("test_get_max_candles", test_get_max_candles(&repo).await);

    time_test!("test_get_last_candle", test_get_last_candle(&repo).await);
}
