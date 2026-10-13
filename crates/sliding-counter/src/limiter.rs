use pacekeeper_core::InMemoryWindowLimiter;

use crate::WindowState;

pub type SlidingCounterRateLimiter<K = String, T = u64> =
    InMemoryWindowLimiter<K, T, WindowState<T>>;
