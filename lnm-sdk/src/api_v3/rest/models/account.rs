use serde::Deserialize;
use uuid::Uuid;

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
}
