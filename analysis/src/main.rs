use std::env;

fn main() {
    let lnm_api_key = env::var("LNM_API_KEY").expect("LNM_API_KEY must be set");
    let lnm_api_secret = env::var("LNM_API_SECRET").expect("LNM_API_SECRET must be set");
    let lnm_api_passphrase =
        env::var("LNM_API_PASSPHRASE").expect("LNM_API_PASSPHRASE must be set");

    println!("LNM_API_KEY: {lnm_api_key}");
    println!("LNM_API_SECRET: {lnm_api_secret}");
    println!("LNM_API_PASSPHRASE: {lnm_api_passphrase}");
}
