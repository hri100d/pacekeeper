use std::{error::Error, fmt, sync::PoisonError};

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ConfigError {
    ZeroCapacity,
    ZeroRefillTokens,
    ZeroRefillPeriod,
    SubmicrosecondPrecision,
    PeriodTooLarge,
    CapacityTooLarge,
    CreditOverflow,
    CreditOutOfRange,
    RefillOutOfRange,
}

#[derive(Debug)]
pub enum BucketError {
    InvalidConfig(ConfigError),
    StateUnavailable,
    ClockWentBackwards,
    ConfigMismatch,
    InvalidRedisState,
    InvalidRedisResponse,
    #[cfg(feature = "redis")]
    Redis(::redis::RedisError),
}

impl fmt::Display for BucketError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidConfig(message) => write!(f, "invalid bucket config: {message}"),
            Self::StateUnavailable => f.write_str("bucket state lock is poisoned"),
            Self::ClockWentBackwards => f.write_str("time moved backwards for this key"),
            Self::ConfigMismatch => f.write_str("the Redis bucket already has a different config"),
            Self::InvalidRedisState => f.write_str("invalid or unsupported Redis bucket state"),
            Self::InvalidRedisResponse => f.write_str("unexpected Redis bucket response"),
            #[cfg(feature = "redis")]
            Self::Redis(error) => write!(f, "Redis operation failed: {error}"),
        }
    }
}

impl Error for BucketError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::InvalidConfig(error) => Some(error),

            #[cfg(feature = "redis")]
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
            Self::CapacityTooLarge => "capacity exceeds the supported range",
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

#[cfg(feature = "redis")]
impl From<::redis::RedisError> for BucketError {
    fn from(error: ::redis::RedisError) -> Self {
        Self::Redis(error)
    }
}
