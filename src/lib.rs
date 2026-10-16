//! Configurable rate limiting with optional algorithms and backends.
//!
//! Enable `fixed`, `sliding-log`, `sliding-counter`, or `bucket` for
//! in-memory limiters. Enable `bucket-redis` or `bucket-redis-cluster`
//! for asynchronous Redis token buckets.
//!
//! No limiter is enabled by default.
//!
//! # Example
//!
//! ```
//! # #[cfg(feature = "fixed")]
//! # {
//! use std::time::Duration;
//! use pacekeeper::{common::Decision, fixed::FixedRateLimiter};
//!
//! let limiter = FixedRateLimiter::<String>::new(
//!     10,
//!     Duration::from_secs(60),
//! ).unwrap();
//!
//! assert!(matches!(
//!     limiter.try_acquire("user:42").unwrap(),
//!     Decision::Allowed { .. },
//! ));
//! # }
//! ```

pub mod common {
    pub use pacekeeper_core::{
        ConfigError, Counter, Decision, Key, LookupKey, RateLimiter, RateLimiterError, WindowConfig,
    };
}

#[cfg(feature = "fixed")]
pub use pacekeeper_fixed as fixed;

#[cfg(feature = "sliding-log")]
pub use pacekeeper_sliding_log as sliding_log;

#[cfg(feature = "sliding-counter")]
pub use pacekeeper_sliding_counter as sliding_counter;

#[cfg(any(
    feature = "bucket",
    feature = "bucket-redis",
    feature = "bucket-redis-cluster",
))]
pub use pacekeeper_bucket as bucket;
