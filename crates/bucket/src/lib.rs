mod config;
mod counter;
mod errors;

#[cfg(feature = "memory")]
mod memory;

#[cfg(feature = "memory")]
mod state;

#[cfg(any(feature = "redis", feature = "redis-cluster"))]
mod redis;

pub use config::BucketConfig;
pub use counter::BucketCounter;
pub use errors::{BucketError, ConfigError};
pub use pacekeeper_core::Decision;

#[cfg(feature = "memory")]
pub use memory::TokenBucket;

#[cfg(any(feature = "redis", feature = "redis-cluster"))]
pub use self::redis::{RedisOptions, RedisTokenBucket};
