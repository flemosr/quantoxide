use lazy_static::lazy_static;
use std::env;

lazy_static! {
    pub static ref LNM_API_BASE_URL: String =
        env::var("LNM_API_BASE_URL").expect("LNM_API_BASE_URL must be set");
    pub static ref LNM_API_KEY: String = env::var("LNM_API_KEY").expect("LNM_API_KEY must be set");
    pub static ref LNM_API_SECRET: String =
        env::var("LNM_API_SECRET").expect("LNM_API_SECRET must be set");
    pub static ref LNM_API_PASSPHRASE: String =
        env::var("LNM_API_PASSPHRASE").expect("LNM_API_PASSPHRASE must be set");
    pub static ref LNM_PRICE_HISTORY_LIMIT: usize = {
        let var = env::var("LNM_PRICE_HISTORY_LIMIT").expect("LNM_PRICE_HISTORY_LIMIT must be set");
        let num = var
            .parse::<usize>()
            .expect("LNM_PRICE_HISTORY_LIMIT must be a valid number");
        assert!(num >= 2, "LNM_PRICE_HISTORY_LIMIT must be at least 2");
        num
    };
    pub static ref LNM_API_COOLDOWN_SEC: u64 = {
        let var = env::var("LNM_API_COOLDOWN_SEC").expect("LNM_API_COOLDOWN_SEC must be set");
        let num = var
            .parse::<u64>()
            .expect("LNM_API_COOLDOWN_SEC must be a valid number");
        num
    };
    pub static ref LNM_API_ERROR_COOLDOWN_SEC: u64 = {
        let var =
            env::var("LNM_API_ERROR_COOLDOWN_SEC").expect("LNM_API_ERROR_COOLDOWN_SEC must be set");
        let num = var
            .parse::<u64>()
            .expect("LNM_API_ERROR_COOLDOWN_SEC must be a valid number");
        num
    };
    pub static ref POSTGRES_DB_URL: String =
        env::var("POSTGRES_DB_URL").expect("POSTGRES_DB_URL must be set");
}
