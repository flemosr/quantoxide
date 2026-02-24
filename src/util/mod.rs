use std::{
    any::Any,
    fmt,
    future::Future,
    ops::{Deref, DerefMut},
    pin::Pin,
    task::{Context, Poll},
};

use tokio::task::{JoinError, JoinHandle};

mod dates;

pub(crate) use dates::DateTimeExt;

/// A type that can not be instantiated
pub(crate) enum Never {}

/// A wrapper around `tokio::task::JoinHandle` that automatically aborts the task
/// when the wrapper is dropped, while allowing access to the handle.
///
/// This is useful for ensuring that spawned tasks are cleaned up when they go out
/// of scope, preventing resource leaks.
///
/// # Important Notes
///
/// - When dropped, this calls `abort()` on the task, which does **not** run destructors
///   or cleanup code. Tasks should be designed to handle abrupt cancellation.
/// - Implements `Deref` and `DerefMut` for transparent access to `JoinHandle` methods
/// - Implements `Future` so it can be awaited just like a regular `JoinHandle`
///
/// # Examples
///
/// ```ignore
/// use tokio::time;
/// use crate::util::AbortOnDropHandle;
///
/// async fn example() {
///     // Task will be aborted when handle goes out of scope
///     let handle = AbortOnDropHandle::from(tokio::spawn(async {
///         loop {
///             // Long-running work...
///             time::sleep(time::Duration::from_secs(1)).await;
///         }
///     }));
///
///     // Can still await the handle if needed
///     // handle.await.unwrap();
/// } // Task is aborted here
/// ```
#[derive(Debug)]
pub(crate) struct AbortOnDropHandle<T>(JoinHandle<T>);

impl<T> From<JoinHandle<T>> for AbortOnDropHandle<T> {
    fn from(handle: JoinHandle<T>) -> Self {
        Self(handle)
    }
}

impl<T> Deref for AbortOnDropHandle<T> {
    type Target = JoinHandle<T>;

    fn deref(&self) -> &Self::Target {
        &self.0
    }
}

impl<T> DerefMut for AbortOnDropHandle<T> {
    fn deref_mut(&mut self) -> &mut Self::Target {
        &mut self.0
    }
}

impl<T> Future for AbortOnDropHandle<T> {
    type Output = Result<T, JoinError>;

    fn poll(mut self: Pin<&mut Self>, cx: &mut Context<'_>) -> Poll<Self::Output> {
        Pin::new(&mut self.0).poll(cx)
    }
}

impl<T> Drop for AbortOnDropHandle<T> {
    fn drop(&mut self) {
        self.0.abort();
    }
}

#[derive(Debug)]
pub struct PanicPayload(String);

impl From<Box<dyn Any + Send>> for PanicPayload {
    fn from(value: Box<dyn Any + Send>) -> Self {
        let panic_msg = if let Some(s) = value.downcast_ref::<String>() {
            s.clone()
        } else if let Some(s) = value.downcast_ref::<&str>() {
            s.to_string()
        } else {
            "unknown panic payload".to_string()
        };

        Self(panic_msg)
    }
}

impl fmt::Display for PanicPayload {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "{}", self.0)
    }
}
