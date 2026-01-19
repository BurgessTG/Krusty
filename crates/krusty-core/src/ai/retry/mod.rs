//! Rate limiting and retry logic
//!
//! Provides exponential backoff with jitter for handling API rate limits and transient errors.
//!
//! NOTE: This is infrastructure for future use. The retry logic is implemented but not yet
//! wired into the streaming calls.

#![allow(dead_code)]

mod backoff;

pub use backoff::{with_retry, IsRetryable, RetryConfig};
