use std::time::Duration;

use crate::{ConfigError, Counter};

#[derive(Debug, Clone)]
pub struct WindowConfig<T: Counter = u64> {
    limit: T,
    window: Duration,
}

impl<T: Counter> WindowConfig<T> {
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

#[cfg(test)]
mod fixed_window_config_tests {
    use super::*;

    #[test]
    fn reject_zero_limit() {
        assert!(matches!(
            WindowConfig::new(0_u8, Duration::from_secs(1)),
            Err(ConfigError::ZeroLimit)
        ))
    }

    #[test]
    fn rejects_zero_window() {
        assert!(matches!(
            WindowConfig::new(1_u8, Duration::ZERO),
            Err(ConfigError::ZeroWindow)
        ))
    }

    #[test]
    fn preserves_valid_configuration() {
        let window = Duration::from_secs(60);
        let config = WindowConfig::new(100u32, window).unwrap();
        assert_eq!(config.limit(), 100);
        assert_eq!(config.window(), window);
    }
}
