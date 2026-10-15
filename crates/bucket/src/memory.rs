use std::{
    borrow::Borrow,
    time::{Duration, Instant},
};

use pacekeeper_core::{Decision, InMemoryLimiter, Key, LookupKey, RateLimiter};

use crate::{BucketConfig, BucketCounter, BucketError, state::BucketState};

pub struct TokenBucket<K: Key = String, C: BucketCounter = u64, R: BucketCounter = u64> {
    inner: InMemoryLimiter<K, BucketConfig<C, R>, BucketState>,
}

impl<K: Key, C: BucketCounter, R: BucketCounter> TokenBucket<K, C, R> {
    pub fn new(
        capacity: C,
        refill_tokens: R,
        refill_period: Duration,
    ) -> Result<Self, BucketError> {
        let config = BucketConfig::new(capacity, refill_tokens, refill_period)?;

        Ok(Self::new_with_config(config))
    }

    pub fn new_with_config(config: BucketConfig<C, R>) -> Self {
        Self {
            inner: InMemoryLimiter::new_with_config(config),
        }
    }

    pub fn config(&self) -> Result<BucketConfig<C, R>, BucketError> {
        self.inner.config().map_err(Into::into)
    }

    pub fn try_acquire<Q>(&self, key: &Q) -> Result<Decision<C>, BucketError>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        self.inner.try_acquire(key).map_err(Into::into)
    }

    pub fn try_acquire_at<Q>(&self, key: &Q, now: Instant) -> Result<Decision<C>, BucketError>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        self.inner.try_acquire_at(key, now).map_err(Into::into)
    }

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
