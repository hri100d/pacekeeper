//! An in-memory sliding-window log rate limiter.
//!
//! Accepted requests are recorded individually and expire when their age
//! reaches the configured window duration.
//!
//! The request limit can change, but the window duration cannot.
//! Explicit timestamps must be nondecreasing for each key.
//!
//! An allowed decision's `reset_after` is the time until the oldest
//! retained request expires, not until the entire allowance is restored.

mod errors;
mod limiter;
mod state;

pub use errors::{SlidingLogWindowError, WindowStateError};
pub use limiter::Limiter as SlidingLogRateLimiter;
