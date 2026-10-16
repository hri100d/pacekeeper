use std::time::Duration;

use crate::{BucketCounter, ConfigError};

pub(crate) const MAX_EXACT: u64 = (1_u64 << 52) - 1;

/// Validated token-bucket capacity and refill rate.
///
/// `C` is the capacity and decision counter type.
/// `R` is the refill token-count type.
///
/// Each type may independently be `u8`, `u16`, `u32`, or `u64`.
/// The same configuration restrictions apply to memory and Redis backends.
///
/// Refill accumulates continuously at `refill_tokens / refill_period`.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct BucketConfig<C: BucketCounter = u64, R: BucketCounter = u64> {
    capacity: C,
    refill_tokens: R,
    refill_period: Duration,
    period_us: u64,
    max_credit: u64,
}

impl<C: BucketCounter, R: BucketCounter> BucketConfig<C, R> {
    /// Creates a validated token-bucket configuration.
    ///
    /// `capacity` is the maximum token balance.
    /// `refill_tokens` specifies the tokens generated per `refill_period`.
    ///
    /// # Errors
    ///
    /// Returns a configuration error when:
    ///
    /// - Capacity, refill tokens, or refill period is zero.
    /// - The refill period is not a whole number of microseconds.
    /// - The period cannot be represented as `u64` microseconds.
    /// - Capacity multiplied by the period in microseconds exceeds `2^52 - 1`.
    /// - Refill tokens exceed `2^52 - 1`.
    ///
    /// The multiplication is also checked for `u64` overflow.
    pub fn new(
        capacity: C,
        refill_tokens: R,
        refill_period: Duration,
    ) -> Result<Self, ConfigError> {
        if capacity == C::zero() {
            return Err(ConfigError::ZeroCapacity);
        }

        if refill_tokens == R::zero() {
            return Err(ConfigError::ZeroRefillTokens);
        }

        if refill_period.is_zero() {
            return Err(ConfigError::ZeroRefillPeriod);
        }

        if !refill_period.subsec_nanos().is_multiple_of(1_000) {
            return Err(ConfigError::SubmicrosecondPrecision);
        }

        let capacity_u64: u64 = capacity.into();
        let refill_u64: u64 = refill_tokens.into();

        if refill_u64 > MAX_EXACT {
            return Err(ConfigError::RefillOutOfRange);
        }

        let period_us =
            u64::try_from(refill_period.as_micros()).map_err(|_| ConfigError::PeriodTooLarge)?;

        let max_credit = capacity_u64
            .checked_mul(period_us)
            .ok_or(ConfigError::CreditOverflow)?;

        if max_credit > MAX_EXACT {
            return Err(ConfigError::CreditOutOfRange);
        }

        Ok(Self {
            capacity,
            refill_tokens,
            refill_period,
            period_us,
            max_credit,
        })
    }

    /// Returns the maximum token balance and initial burst allowance.
    pub fn capacity(&self) -> C {
        self.capacity
    }

    /// Returns the number of tokens generated per refill period.
    pub fn refill_tokens(&self) -> R {
        self.refill_tokens
    }

    /// Returns the duration used to express the refill rate.
    pub fn refill_period(&self) -> Duration {
        self.refill_period
    }
}

#[cfg(any(feature = "memory", feature = "redis", feature = "redis-cluster"))]
impl<C: BucketCounter, R: BucketCounter> BucketConfig<C, R> {
    pub(crate) fn refill_tokens_u64(&self) -> u64 {
        self.refill_tokens.into()
    }

    pub(crate) fn period_us(&self) -> u64 {
        self.period_us
    }

    pub(crate) fn max_credit(&self) -> u64 {
        self.max_credit
    }
}
