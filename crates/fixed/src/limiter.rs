use pacekeeper_core::InMemoryWindowLimiter;

use crate::WindowState;

pub type FixedRateLimiter<K = String, T = u64> = InMemoryWindowLimiter<K, T, WindowState<T>>;
