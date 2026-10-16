//! Shared configuration, decisions, traits, and in-memory engines.
//!
//! Applications usually use an algorithm crate or the `pacekeeper`
//! entry-point crate. Custom algorithms can implement `State` or
//! `InMemoryState` and reuse the corresponding engine.
//!
//! `RateLimiter` describes synchronous limiters. Asynchronous Redis
//! implementations use their own inherent methods.
//!
//! Decision durations have algorithm-specific meanings.

mod config;
mod counter;
mod decision;
mod errors;
mod key;
mod limiter;
mod memory;
mod state;

pub use config::WindowConfig;
pub use counter::Counter;
pub use decision::Decision;
pub use errors::{ConfigError, InMemoryError, RateLimiterError};
pub use key::{Key, LookupKey};
pub use limiter::{InMemoryWindowLimiter, RateLimiter};
pub use memory::InMemoryLimiter;
pub use state::{InMemoryState, State};
