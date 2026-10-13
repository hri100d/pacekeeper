use std::{error::Error, fmt, sync::PoisonError};

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum ConfigError {
    ZeroLimit,
    ZeroWindow,
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ZeroLimit => {
                write!(f, "the request limit must be greater than zero")
            }
            Self::ZeroWindow => {
                write!(f, "the window duration must be greater than zero")
            }
        }
    }
}

impl Error for ConfigError {}

#[derive(Debug)]
pub enum RateLimiterError<E> {
    StateUnavailable,
    StateError(E),
    ConfigError(ConfigError),
}

impl<E: fmt::Display> fmt::Display for RateLimiterError<E> {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::StateUnavailable => {
                write!(f, "rate limiter state is unavailable: mutex poisoned")
            }
            Self::StateError(error) => {
                write!(f, "rate limiter state error: {error}")
            }
            Self::ConfigError(error) => {
                write!(f, "invalid rate limiter configuration: {error}")
            }
        }
    }
}

impl<E: Error + 'static> Error for RateLimiterError<E> {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::StateUnavailable => None,
            Self::StateError(error) => Some(error),
            Self::ConfigError(error) => Some(error),
        }
    }
}

impl<E> From<ConfigError> for RateLimiterError<E> {
    fn from(error: ConfigError) -> Self {
        Self::ConfigError(error)
    }
}

impl<E, G> From<PoisonError<G>> for RateLimiterError<E> {
    fn from(_: PoisonError<G>) -> Self {
        Self::StateUnavailable
    }
}
