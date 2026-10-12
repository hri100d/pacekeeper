use std::sync::PoisonError;

#[derive(Debug)]
pub enum WindowError {
    DurationOverflow,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    ZeroLimit,
    ZeroWindow,
}

#[derive(Debug)]
pub enum FixedWindowError {
    StateUnavailable,
    WindowError(WindowError),
    ConfigError(ConfigError),
}

impl From<WindowError> for FixedWindowError {
    fn from(error: WindowError) -> Self {
        Self::WindowError(error)
    }
}

impl From<ConfigError> for FixedWindowError {
    fn from(error: ConfigError) -> Self {
        Self::ConfigError(error)
    }
}

impl<T> From<PoisonError<T>> for FixedWindowError {
    fn from(_: PoisonError<T>) -> Self {
        Self::StateUnavailable
    }
}
