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
