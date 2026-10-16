use std::time::Duration;

use crate::Counter;

/// The result of attempting to acquire one request allowance.
///
/// An allowed decision has already consumed the allowance.
/// A denied decision has not consumed an additional allowance.
///
/// Returned durations assume unchanged configuration and no intervening
/// accepted requests. They do not reserve future capacity.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision<T: Counter = u64> {
    /// The request was accepted and counted.
    Allowed {
        /// Additional whole requests or tokens currently available.
        ///
        /// For a sliding counter, this is based on estimated usage.
        remaining: T,

        /// An algorithm-specific reset or refill duration.
        ///
        /// - Fixed window: time until the current window expires.
        /// - Sliding log: time until the oldest retained request expires.
        /// - Sliding counter: time until the current counter window ends.
        /// - Token bucket: time until the bucket becomes full.
        reset_after: Duration,
    },

    /// The request was rejected without consuming another allowance.
    Denied {
        /// Calculated wait until another request can be accepted.
        retry_after: Duration,
    },
}
