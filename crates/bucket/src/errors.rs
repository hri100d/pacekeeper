use std::{error::Error, fmt, sync::PoisonError};

/// An invalid token-bucket configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    /// Capacity was zero.
    ZeroCapacity,

    /// Refill token count was zero.
    ZeroRefillTokens,

    /// Refill period was zero.
    ZeroRefillPeriod,

    /// Refill period was not a whole number of microseconds.
    SubmicrosecondPrecision,

    /// Refill period did not fit in `u64` microseconds.
    PeriodTooLarge,

    /// Capacity multiplied by the period overflowed `u64`.
    CreditOverflow,

    /// Scaled capacity exceeded `2^52 - 1`.
    CreditOutOfRange,

    /// Refill token count exceeded `2^52 - 1`.
    RefillOutOfRange,
}

/// A failure to configure or operate a token bucket.
///
/// Normal rate-limit rejection is represented by `Decision::Denied`,
/// not by this error type.
#[derive(Debug)]
pub enum BucketError {
    /// Configuration validation failed.
    InvalidConfig(ConfigError),

    /// The in-memory state mutex was poisoned.
    StateUnavailable,

    /// The supplied or server timestamp preceded stored state.
    ClockWentBackwards,

    /// A configuration was incompatible with the existing bucket policy.
    ConfigMismatch,

    /// Stored Redis bucket fields were missing or invalid.
    InvalidRedisState,

    /// Redis returned a response that violated the expected format or bounds.
    InvalidRedisResponse,

    /// An underlying Redis operation failed.
    #[cfg(any(feature = "redis", feature = "redis-cluster"))]
    Redis(::redis::RedisError),
}

impl fmt::Display for BucketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(f, "invalid bucket config: {message}"),
            Self::StateUnavailable => f.write_str("bucket state lock is poisoned"),
            Self::ClockWentBackwards => f.write_str("time moved backwards for this key"),
            Self::ConfigMismatch => f.write_str("the bucket already has a different configuration"),
            Self::InvalidRedisState => f.write_str("invalid or unsupported Redis bucket state"),
            Self::InvalidRedisResponse => f.write_str("unexpected Redis bucket response"),
            #[cfg(any(feature = "redis", feature = "redis-cluster"))]
            Self::Redis(error) => write!(f, "Redis operation failed: {error}"),
        }
    }
}

impl Error for BucketError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidConfig(error) => Some(error),

            #[cfg(any(feature = "redis", feature = "redis-cluster"))]
            Self::Redis(error) => Some(error),

            _ => None,
        }
    }
}

impl fmt::Display for ConfigError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        let message = match self {
            Self::ZeroCapacity => "capacity must be positive",
            Self::ZeroRefillTokens => "refill tokens must be positive",
            Self::ZeroRefillPeriod => "refill period must be positive",
            Self::SubmicrosecondPrecision => "refill period must use whole microseconds",
            Self::PeriodTooLarge => "refill period does not fit in u64 microseconds",
            Self::CreditOverflow => "capacity multiplied by period overflows u64",
            Self::CreditOutOfRange => "scaled capacity exceeds the supported range",
            Self::RefillOutOfRange => "refill tokens exceed the supported range",
        };

        f.write_str(message)
    }
}

impl Error for ConfigError {}

impl<G> From<PoisonError<G>> for BucketError {
    fn from(_: PoisonError<G>) -> Self {
        Self::StateUnavailable
    }
}

impl From<ConfigError> for BucketError {
    fn from(error: ConfigError) -> Self {
        Self::InvalidConfig(error)
    }
}

#[cfg(any(feature = "redis", feature = "redis-cluster"))]
impl From<::redis::RedisError> for BucketError {
    fn from(error: ::redis::RedisError) -> Self {
        match error.code() {
            Some("PACEKEEPER_CONFIG_MISMATCH") => Self::ConfigMismatch,
            Some("PACEKEEPER_INVALID_STATE") => Self::InvalidRedisState,
            Some("PACEKEEPER_CLOCK_BACKWARDS") => Self::ClockWentBackwards,
            _ => Self::Redis(error),
        }
    }
}

#[cfg(feature = "memory")]
impl From<pacekeeper_core::InMemoryError<BucketError>> for BucketError {
    fn from(error: pacekeeper_core::InMemoryError<BucketError>) -> Self {
        match error {
            pacekeeper_core::InMemoryError::StateUnavailable => Self::StateUnavailable,

            pacekeeper_core::InMemoryError::StateError(error) => error,
        }
    }
}
