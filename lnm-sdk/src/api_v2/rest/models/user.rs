use std::fmt;

use serde::Deserialize;
use serde_json::Value;
use uuid::Uuid;

/// User role within the LN Markets platform.
#[derive(Debug, Deserialize, PartialEq)]
#[serde(rename_all = "lowercase")]
pub enum UserRole {
    User,
    Moderator,
    Operator,
    Admin,
}

impl fmt::Display for UserRole {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let role_str = match self {
            UserRole::User => "User",
            UserRole::Moderator => "Moderator",
            UserRole::Operator => "Operator",
            UserRole::Admin => "Admin",
        };
        write!(f, "{}", role_str)
    }
}

/// User account information from LN Markets.
///
/// Contains comprehensive details about a user's account including balance, settings, security
/// configurations, and platform preferences.
///
/// # Examples
///
/// ```no_run
/// # #[allow(deprecated)]
/// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::api_v2::models::User;
///
/// let user: User = rest.user.get_user().await?;
///
/// println!("User: {}", user.username());
/// println!("Balance: {} sats", user.balance());
/// # Ok(())
/// # }
/// ```
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
    metrics: Option<Value>, // As of Nov 10 2025, format not specified in the LNM's docs
}

impl User {
    /// Returns the user's unique identifier.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    ///
    /// println!("User ID: {}", user.uid());
    /// # Ok(())
    /// # }
    /// ```
    pub fn uid(&self) -> &Uuid {
        &self.uid
    }

    /// Returns the user's role on the platform.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// # use lnm_sdk::api_v2::models::UserRole;
    /// let user = rest.user.get_user().await?;
    ///
    /// if matches!(user.role(), UserRole::Admin) {
    ///     println!("User has admin privileges");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn role(&self) -> &UserRole {
        &self.role
    }

    /// Returns the user's balance in satoshis.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    ///
    /// println!("Balance: {} sats", user.balance());
    /// # Ok(())
    /// # }
    /// ```
    pub fn balance(&self) -> u64 {
        self.balance
    }

    /// Returns the user's username.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    /// println!("Welcome, {}!", user.username());
    /// # Ok(())
    /// # }
    /// ```
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Returns the user's synthetic USD balance.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    ///
    /// println!("Synthetic USD balance: {}", user.synthetic_usd_balance());
    /// # Ok(())
    /// # }
    /// ```
    pub fn synthetic_usd_balance(&self) -> u64 {
        self.synthetic_usd_balance
    }

    /// Returns the user's linking public key, if set.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    ///
    /// if let Some(key) = user.linkingpublickey() {
    ///     println!("Linking public key: {}", key);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn linkingpublickey(&self) -> Option<&str> {
        self.linkingpublickey.as_deref()
    }

    /// Returns whether the user has opted to show their position on the leaderboard.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    ///
    /// if user.show_leaderboard() {
    ///     println!("User is visible on leaderboard");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn show_leaderboard(&self) -> bool {
        self.show_leaderboard
    }

    /// Returns the user's email address, if set.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    ///
    /// if let Some(email) = user.email() {
    ///     println!("Email: {}", email);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn email(&self) -> Option<&str> {
        self.email.as_deref()
    }

    /// Returns whether the user's email address has been confirmed.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    ///
    /// if !user.email_confirmed() {
    ///     println!("Email not confirmed");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn email_confirmed(&self) -> bool {
        self.email_confirmed
    }

    /// Returns whether the user has Taproot addresses enabled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    ///
    /// if user.use_taproot_addresses() {
    ///     println!("Using Taproot addresses");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn use_taproot_addresses(&self) -> bool {
        self.use_taproot_addresses
    }

    /// Returns the user's account type.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    ///
    /// println!("Account type: {}", user.account_type());
    /// # Ok(())
    /// # }
    /// ```
    pub fn account_type(&self) -> &str {
        &self.account_type
    }

    /// Returns whether automatic withdrawal is enabled for the user.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    ///
    /// if user.auto_withdraw_enabled() {
    ///     println!("Auto-withdrawal is enabled");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn auto_withdraw_enabled(&self) -> bool {
        self.auto_withdraw_enabled
    }

    /// Returns the Lightning address for automatic withdrawals, if configured.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    ///
    /// if let Some(address) = user.auto_withdraw_lightning_address() {
    ///     println!("Auto-withdraw address: {}", address);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn auto_withdraw_lightning_address(&self) -> Option<&str> {
        self.auto_withdraw_lightning_address.as_deref()
    }

    /// Returns whether Time-based One-Time Password (TOTP) authentication is enabled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    ///
    /// if user.totp_enabled() {
    ///     println!("TOTP 2FA is enabled");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn totp_enabled(&self) -> bool {
        self.totp_enabled
    }

    /// Returns whether WebAuthn authentication is enabled.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    ///
    /// if user.webauthn_enabled() {
    ///     println!("WebAuthn 2FA is enabled");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn webauthn_enabled(&self) -> bool {
        self.webauthn_enabled
    }

    /// Returns the user's fee tier level.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    ///
    /// println!("Fee tier: {}", user.fee_tier());
    /// # Ok(())
    /// # }
    /// ```
    pub fn fee_tier(&self) -> u8 {
        self.fee_tier
    }

    /// Returns the user's metrics data, if available.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # #[allow(deprecated)]
    /// # async fn example(rest: lnm_sdk::api_v2::RestClient) -> Result<(), Box<dyn std::error::Error>> {
    /// let user = rest.user.get_user().await?;
    ///
    /// if let Some(metrics) = user.metrics() {
    ///     println!("Metrics: {:?}", metrics);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn metrics(&self) -> Option<&serde_json::Value> {
        self.metrics.as_ref()
    }

    pub fn as_data_str(&self) -> String {
        let mut data_str = format!(
            "uid: {}\nusername: {}\nrole: {}\nbalance: {}\nsynthetic_usd_balance: {}\naccount_type: {}\nfee_tier: {}\nshow_leaderboard: {}\nemail_confirmed: {}\nuse_taproot_addresses: {}\nauto_withdraw_enabled: {}\ntotp_enabled: {}\nwebauthn_enabled: {}",
            self.uid,
            self.username,
            self.role,
            self.balance,
            self.synthetic_usd_balance,
            self.account_type,
            self.fee_tier,
            self.show_leaderboard,
            self.email_confirmed,
            self.use_taproot_addresses,
            self.auto_withdraw_enabled,
            self.totp_enabled,
            self.webauthn_enabled
        );

        if let Some(email) = &self.email {
            data_str.push_str(&format!("\nemail: {email}"));
        }
        if let Some(linking_key) = &self.linkingpublickey {
            data_str.push_str(&format!("\nlinkingpublickey: {linking_key}"));
        }
        if let Some(ln_address) = &self.auto_withdraw_lightning_address {
            data_str.push_str(&format!("\nauto_withdraw_lightning_address: {ln_address}"));
        }

        data_str
    }
}

impl fmt::Display for User {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "User:")?;
        for line in self.as_data_str().lines() {
            write!(f, "\n  {line}")?;
        }
        Ok(())
    }
}
