use std::{
    borrow::Borrow,
    time::{Duration, Instant},
};

use pacekeeper_core::{Decision, InMemoryLimiter, Key, LookupKey, RateLimiter};

use crate::{BucketConfig, BucketCounter, BucketError, state::BucketState};

/// An in-memory token bucket with independent state per key.
///
/// Available with the `memory` feature.
///
/// New buckets start full. Each allowed acquisition consumes one token.
/// Refill accumulates continuously and is capped at capacity.
///
/// `K` is the owned key type, `C` is the capacity counter type, and
/// `R` is the refill counter type.
///
/// The limiter retains idle key entries until explicitly removed.
/// Share an instance with `Arc` when usage should be shared across threads.
pub struct TokenBucket<K: Key = String, C: BucketCounter = u64, R: BucketCounter = u64> {
    inner: InMemoryLimiter<K, BucketConfig<C, R>, BucketState>,
}

impl<K: Key, C: BucketCounter, R: BucketCounter> TokenBucket<K, C, R> {
    /// Creates an empty map of buckets with the specified policy.
    ///
    /// Each key's bucket is initialized at full capacity on first access.
    ///
    /// # Errors
    ///
    /// Returns `BucketError::InvalidConfig` if the configuration is invalid.
    pub fn new(
        capacity: C,
        refill_tokens: R,
        refill_period: Duration,
    ) -> Result<Self, BucketError> {
        let config = BucketConfig::new(capacity, refill_tokens, refill_period)?;

        Ok(Self::new_with_config(config))
    }

    /// Creates a limiter from a validated token-bucket configuration.
    ///
    /// Each key's bucket is initialized at full capacity on first access.
    pub fn new_with_config(config: BucketConfig<C, R>) -> Self {
        Self {
            inner: InMemoryLimiter::new_with_config(config),
        }
    }

    /// Returns a copy of the limiter's configuration.
    ///
    /// # Errors
    ///
    /// Returns `BucketError::StateUnavailable` if the mutex is poisoned.
    pub fn config(&self) -> Result<BucketConfig<C, R>, BucketError> {
        self.inner.config().map_err(Into::into)
    }

    /// Attempts to consume one token using the current monotonic time.
    ///
    /// An allowed result has already consumed the token.
    /// A denied result preserves fractional refill progress.
    ///
    /// # Errors
    ///
    /// Returns an error if the mutex is poisoned or time precedes a timestamp
    /// previously supplied for this key.
    pub fn try_acquire<Q>(&self, key: &Q) -> Result<Decision<C>, BucketError>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        self.inner.try_acquire(key).map_err(Into::into)
    }

    /// Attempts to consume one token at an explicit monotonic timestamp.
    ///
    /// Timestamps for a key must be nondecreasing in processing order.
    /// This method is useful for deterministic tests.
    ///
    /// # Errors
    ///
    /// Returns `BucketError::ClockWentBackwards` for an earlier timestamp,
    /// without changing that bucket's state.
    /// Returns `BucketError::StateUnavailable` if the mutex is poisoned.
    pub fn try_acquire_at<Q>(&self, key: &Q, now: Instant) -> Result<Decision<C>, BucketError>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        self.inner.try_acquire_at(key, now).map_err(Into::into)
    }

    /// Removes a key's bucket and returns whether it existed.
    ///
    /// The next request for the key starts from a full bucket.
    ///
    /// # Errors
    ///
    /// Returns `BucketError::StateUnavailable` if the mutex is poisoned.
    pub fn remove_key<Q>(&self, key: &Q) -> Result<bool, BucketError>
    where
        K: Borrow<Q>,
        Q: Key + ?Sized,
    {
        self.inner.remove_key(key).map_err(Into::into)
    }
}

impl<K, Q, C, R> RateLimiter<Q, C> for TokenBucket<K, C, R>
where
    K: Key + Borrow<Q>,
    Q: LookupKey<K> + ?Sized,
    C: BucketCounter,
    R: BucketCounter,
{
    type Error = BucketError;

    fn try_acquire(&self, key: &Q) -> Result<Decision<C>, Self::Error> {
        TokenBucket::try_acquire(self, key)
    }

    fn try_acquire_at(&self, key: &Q, now: Instant) -> Result<Decision<C>, Self::Error> {
        TokenBucket::try_acquire_at(self, key, now)
    }

    fn remove_key(&self, key: &Q) -> Result<bool, Self::Error> {
        TokenBucket::remove_key(self, key)
    }
}
