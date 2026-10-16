//! An in-memory fixed-window rate limiter.
//!
//! Each key has an independent counter and window start. An acquisition
//! after expiration starts a new window at that acquisition's timestamp.
//!
//! Configuration updates preserve usage. Idle key entries remain stored
//! until explicitly removed.
//!
//! # Example
//!
//! ```
//! use std::time::Duration;
//! use pacekeeper_fixed::FixedRateLimiter;
//!
//! let limiter = FixedRateLimiter::<String>::new(
//!     10,
//!     Duration::from_secs(60),
//! ).unwrap();
//!
//! let decision = limiter.try_acquire("user:42").unwrap();
//! ```

mod errors;
mod limiter;
mod state;

pub use errors::{FixedWindowError, WindowStateError};
pub use limiter::Limiter as FixedRateLimiter;
