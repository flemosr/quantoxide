use lazy_static::lazy_static;
use std::env;

lazy_static! {
    pub static ref LNM_API_DOMAIN: String =
        env::var("LNM_API_DOMAIN").expect("LNM_API_DOMAIN must be set");
    pub static ref LNM_API_KEY: String = env::var("LNM_API_KEY").expect("LNM_API_KEY must be set");
    pub static ref LNM_API_SECRET: String =
        env::var("LNM_API_SECRET").expect("LNM_API_SECRET must be set");
    pub static ref LNM_API_PASSPHRASE: String =
        env::var("LNM_API_PASSPHRASE").expect("LNM_API_PASSPHRASE must be set");
    pub static ref LNM_PRICE_HISTORY_BATCH_ENTRIES: usize = {
        let num = env::var("LNM_PRICE_HISTORY_BATCH_ENTRIES")
            .expect("LNM_PRICE_HISTORY_BATCH_ENTRIES must be set")
            .parse::<usize>()
            .expect("LNM_PRICE_HISTORY_BATCH_ENTRIES must be a valid number");
        assert!(
            num >= 2,
            "LNM_PRICE_HISTORY_BATCH_ENTRIES must be at least 2"
        );
        num
    };
    pub static ref LNM_API_COOLDOWN_SEC: u64 = {
        env::var("LNM_API_COOLDOWN_SEC")
            .expect("LNM_API_COOLDOWN_SEC must be set")
            .parse::<u64>()
            .expect("LNM_API_COOLDOWN_SEC must be a valid number")
    };
    pub static ref LNM_API_ERROR_MAX_TRIALS: u32 = {
        env::var("LNM_API_ERROR_MAX_TRIALS")
            .expect("LNM_API_ERROR_MAX_TRIALS must be set")
            .parse::<u32>()
            .expect("LNM_API_ERROR_MAX_TRIALS must be a valid number")
    };
    pub static ref LNM_API_ERROR_COOLDOWN_SEC: u64 = {
        env::var("LNM_API_ERROR_COOLDOWN_SEC")
            .expect("LNM_API_ERROR_COOLDOWN_SEC must be set")
            .parse::<u64>()
            .expect("LNM_API_ERROR_COOLDOWN_SEC must be a valid number")
    };
    pub static ref SYNC_HISTORY_REACH_HOURS: u64 = {
        let num = env::var("SYNC_HISTORY_REACH_HOURS")
            .expect("SYNC_HISTORY_REACH_HOURS must be set")
            .parse::<u64>()
            .expect("SYNC_HISTORY_REACH_HOURS must be a valid number");
        assert!(num >= 1, "SYNC_HISTORY_REACH_HOURS must be at least 1");
        num
    };
    pub static ref POSTGRES_DB_URL: String =
        env::var("POSTGRES_DB_URL").expect("POSTGRES_DB_URL must be set");
}
