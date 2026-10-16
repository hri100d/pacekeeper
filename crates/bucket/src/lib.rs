//! Token buckets with optional memory and Redis backends.
//!
//! Enable `memory` for `TokenBucket`, `redis` for standalone Redis,
//! or `redis-cluster` for Redis Cluster. Features may be combined.
//!
//! Buckets start full. Each allowed acquisition consumes one token.
//! Refill accumulates continuously and never exceeds capacity.
//!
//! An allowed decision's `reset_after` is the time until full capacity.
//! A denied decision's `retry_after` is the time until one token is available.
//!
//! Redis acquisitions use server time and execute atomically through Lua.
//! Backend failures are returned to the caller without an automatic
//! fallback policy.

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
