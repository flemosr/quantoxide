use async_trait::async_trait;

/// Methods for interacting with [LNM's v3 API]'s REST Futures endpoints.
///
/// This trait is sealed and not meant to be implemented outside of `lnm-sdk`.
///
/// [LNM's v3 API]: https://docs.lnmarkets.com/api/#overview
#[async_trait]
pub trait FuturesIsolatedRepository: crate::sealed::Sealed + Send + Sync {}
