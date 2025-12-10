use std::fmt;

use serde::Deserialize;
use uuid::Uuid;

/// LN Markets account information.
///
/// # Examples
///
/// ```no_run
/// # async fn example(rest_api: lnm_sdk::api_v3::RestClient) -> Result<(), Box<dyn std::error::Error>> {
/// use lnm_sdk::api_v3::models::Account;
///
/// let account: Account = rest_api
///     .account
///     .get_account()
///     .await?;
///
/// println!("Account ID: {}", account.id());
/// println!("Username: {}", account.username());
/// println!("Email: {}", account.email());
/// println!("Balance: {} sats", account.balance());
/// println!("Synthetic USD balance: {} cents", account.synthetic_usd_balance());
/// println!("Fee tier: {}", account.fee_tier());
///
/// if let Some(public_key) = account.linking_public_key() {
///     println!("Linking public key: {}", public_key);
/// }
/// # Ok(())
/// # }
/// ```
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Account {
    id: Uuid,
    username: String,
    email: String,
    synthetic_usd_balance: u64,
    balance: u64,
    fee_tier: u64,
    linking_public_key: Option<String>,
}

impl Account {
    /// Returns the unique identifier for this account.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(account: lnm_sdk::api_v3::models::Account) -> Result<(), Box<dyn std::error::Error>> {
    /// let account_id = account.id();
    ///
    /// println!("Account ID: {}", account_id);
    /// # Ok(())
    /// # }
    /// ```
    pub fn id(&self) -> Uuid {
        self.id
    }

    /// Returns the username associated with this account.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(account: lnm_sdk::api_v3::models::Account) -> Result<(), Box<dyn std::error::Error>> {
    /// let username = account.username();
    ///
    /// println!("Username: {}", username);
    /// # Ok(())
    /// # }
    /// ```
    pub fn username(&self) -> &str {
        &self.username
    }

    /// Returns the email address associated with this account.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(account: lnm_sdk::api_v3::models::Account) -> Result<(), Box<dyn std::error::Error>> {
    /// let email = account.email();
    ///
    /// println!("Email: {}", email);
    /// # Ok(())
    /// # }
    /// ```
    pub fn email(&self) -> &str {
        &self.email
    }

    /// Returns the synthetic USD balance (in cents).
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(account: lnm_sdk::api_v3::models::Account) -> Result<(), Box<dyn std::error::Error>> {
    /// let synthetic_balance = account.synthetic_usd_balance();
    ///
    /// println!("Synthetic USD balance: {} cents", synthetic_balance);
    /// # Ok(())
    /// # }
    /// ```
    pub fn synthetic_usd_balance(&self) -> u64 {
        self.synthetic_usd_balance
    }

    /// Returns the account balance in satoshis.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(account: lnm_sdk::api_v3::models::Account) -> Result<(), Box<dyn std::error::Error>> {
    /// let balance = account.balance();
    ///
    /// println!("Balance: {} sats", balance);
    /// # Ok(())
    /// # }
    /// ```
    pub fn balance(&self) -> u64 {
        self.balance
    }

    /// Returns the fee tier for this account.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(account: lnm_sdk::api_v3::models::Account) -> Result<(), Box<dyn std::error::Error>> {
    /// let fee_tier = account.fee_tier();
    ///
    /// println!("Fee tier: {}", fee_tier);
    /// # Ok(())
    /// # }
    /// ```
    pub fn fee_tier(&self) -> u64 {
        self.fee_tier
    }

    /// Returns the linking public key for this account.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # fn example(account: lnm_sdk::api_v3::models::Account) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(public_key) = account.linking_public_key() {
    ///     println!("Linking public key: {}", public_key);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn linking_public_key(&self) -> Option<&String> {
        self.linking_public_key.as_ref()
    }

    pub fn as_data_str(&self) -> String {
        let mut data_str = format!(
            "id: {}\nusername: {}\nemail: {}\nbalance: {}\nsynthetic_usd_balance: {}\nfee_tier: {}",
            self.id,
            self.username,
            self.email,
            self.balance,
            self.synthetic_usd_balance,
            self.fee_tier
        );

        if let Some(linking_key) = &self.linking_public_key {
            data_str.push_str(&format!("\nlinking_public_key: {linking_key}"));
        }

        data_str
    }
}

impl fmt::Display for Account {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "Account:")?;
        for line in self.as_data_str().lines() {
            write!(f, "\n  {line}")?;
        }
        Ok(())
    }
}
