use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    User,
    Moderator,
    Operator,
    Admin,
}

#[derive(Debug, Deserialize)]
pub struct User {
    uid: Uuid,
    role: UserRole,
    balance: u64,
    username: String,
    synthetic_usd_balance: u64,
    linkingpublickey: Option<String>,
    show_leaderboard: bool,
    email: Option<String>,
    email_confirmed: bool,
    use_taproot_addresses: bool,
    account_type: String,
    auto_withdraw_enabled: bool,
    auto_withdraw_lightning_address: Option<String>,
    totp_enabled: bool,
    webauthn_enabled: bool,
    fee_tier: u8,
    metrics: Option<Value>, // Format not specified in the docs
}

impl User {
    pub fn uid(&self) -> &Uuid {
        &self.uid
    }

    pub fn role(&self) -> &UserRole {
        &self.role
    }

    pub fn balance(&self) -> u64 {
        self.balance
    }

    pub fn username(&self) -> &str {
        &self.username
    }

    pub fn synthetic_usd_balance(&self) -> u64 {
        self.synthetic_usd_balance
    }

    pub fn linkingpublickey(&self) -> Option<&str> {
        self.linkingpublickey.as_deref()
    }

    pub fn show_leaderboard(&self) -> bool {
        self.show_leaderboard
    }

    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    pub fn email_confirmed(&self) -> bool {
        self.email_confirmed
    }

    pub fn use_taproot_addresses(&self) -> bool {
        self.use_taproot_addresses
    }

    pub fn account_type(&self) -> &str {
        &self.account_type
    }

    pub fn auto_withdraw_enabled(&self) -> bool {
        self.auto_withdraw_enabled
    }

    pub fn auto_withdraw_lightning_address(&self) -> Option<&str> {
        self.auto_withdraw_lightning_address.as_deref()
    }

    pub fn totp_enabled(&self) -> bool {
        self.totp_enabled
    }

    pub fn webauthn_enabled(&self) -> bool {
        self.webauthn_enabled
    }

    pub fn fee_tier(&self) -> u8 {
        self.fee_tier
    }

    pub fn metrics(&self) -> Option<&serde_json::Value> {
        self.metrics.as_ref()
    }
}
