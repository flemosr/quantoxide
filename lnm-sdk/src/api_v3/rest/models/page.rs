use chrono::{DateTime, Utc};
use serde::Deserialize;

/// Generic paginated response structure.
///
/// Contains a vector of items and an optional cursor for fetching the next page.
#[derive(Deserialize, Debug, Clone)]
#[serde(rename_all = "camelCase")]
pub struct Page<I> {
    data: Vec<I>,
    next_cursor: Option<DateTime<Utc>>,
}

impl<I> Page<I> {
    /// Vector of items in this page.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use lnm_sdk::api_v3::models::{Page, Trade};
    /// # fn example(page: Page<Trade>) -> Result<(), Box<dyn std::error::Error>> {
    /// for item in page.data() {
    ///     println!("item: {:?}", item);
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn data(&self) -> &Vec<I> {
        &self.data
    }

    /// Cursor that can be used to fetch the next page of results. `None` if there are no more
    /// results.
    ///
    /// # Examples
    ///
    /// ```no_run
    /// # use lnm_sdk::api_v3::models::{Page, Trade};
    /// # fn example(page: Page<Trade>) -> Result<(), Box<dyn std::error::Error>> {
    /// if let Some(cursor) = page.next_cursor() {
    ///     println!("More items can be fetched using cursor: {cursor}");
    /// } else {
    ///     println!("There are no more items available.");
    /// }
    /// # Ok(())
    /// # }
    /// ```
    pub fn next_cursor(&self) -> Option<DateTime<Utc>> {
        self.next_cursor
    }
}

impl<I> From<Page<I>> for Vec<I> {
    fn from(value: Page<I>) -> Self {
        value.data
    }
}
