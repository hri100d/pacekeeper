use std::time::Duration;

use pacekeeper_core::Counter;

use crate::ConfigError;

#[derive(Debug, Clone)]
pub struct FixedWindowConfig<T: Counter = u64> {
    limit: T,
    window: Duration,
}

impl<T: Counter> FixedWindowConfig<T> {
    pub fn new(limit: T, window: Duration) -> Result<Self, ConfigError> {
        if limit == T::zero() {
            return Err(ConfigError::ZeroLimit);
        }

        if window.is_zero() {
            return Err(ConfigError::ZeroWindow);
        }

        Ok(Self { limit, window })
    }

    pub fn limit(&self) -> T {
        self.limit
    }

    pub fn window(&self) -> Duration {
        self.window
    }
}
