//! An in-memory approximation of sliding-window rate limiting.
//!
//! Usage combines the current window's count with a weighted contribution
//! from the previous window. That contribution is rounded upward.
//!
//! The request limit and window duration are fixed at construction.
//!
//! An allowed decision's `reset_after` identifies the current window
//! boundary. Previous usage may continue to affect the following window.

mod errors;
mod limiter;
mod state;

pub use errors::{SlidingCounterWindowError, WindowStateError};
pub use limiter::Limiter as SlidingCounterRateLimiter;
