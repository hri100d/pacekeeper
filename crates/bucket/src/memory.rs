use std::{
    borrow::Borrow,
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use pacekeeper_core::{Decision, Key, LookupKey, RateLimiter};

use crate::{BucketConfig, BucketCounter, BucketError, state::BucketState};

pub struct TokenBucket<K: Key = String, C: BucketCounter = u64, R: BucketCounter = u64> {
    config: BucketConfig<C, R>,
    buckets: Mutex<HashMap<K, BucketState>>,
}

impl<K: Key, C: BucketCounter, R: BucketCounter> TokenBucket<K, C, R> {
    pub fn new(
        capacity: C,
        refill_tokens: R,
        refill_period: Duration,
    ) -> Result<Self, BucketError> {
        Ok(Self::new_with_config(BucketConfig::new(
            capacity,
            refill_tokens,
            refill_period,
        )?))
    }

    pub fn new_with_config(config: BucketConfig<C, R>) -> Self {
        Self {
            config,
            buckets: Mutex::new(HashMap::new()),
        }
    }

    pub fn config(&self) -> BucketConfig<C, R> {
        self.config
    }

    pub fn try_acquire<Q>(&self, key: &Q) -> Result<Decision<C>, BucketError>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        self.acquire_with_clock(key, Instant::now)
    }

    pub fn try_acquire_at<Q>(&self, key: &Q, now: Instant) -> Result<Decision<C>, BucketError>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        self.acquire_with_clock(key, || now)
    }

    fn acquire_with_clock<Q>(
        &self,
        key: &Q,
        read_time: impl FnOnce() -> Instant,
    ) -> Result<Decision<C>, BucketError>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        let mut buckets = self.buckets.lock()?;
        let now = read_time();

        if let Some(state) = buckets.get_mut(key) {
            return state.try_accept(now, &self.config);
        }

        let state = buckets
            .entry(key.to_owned())
            .or_insert_with(|| BucketState::new(&self.config, now));

        state.try_accept(now, &self.config)
    }

    pub fn remove_key<Q>(&self, key: &Q) -> Result<bool, BucketError>
    where
        K: Borrow<Q>,
        Q: Key + ?Sized,
    {
        Ok(self.buckets.lock()?.remove(key).is_some())
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
        self.try_acquire(key)
    }

    fn try_acquire_at(&self, key: &Q, now: Instant) -> Result<Decision<C>, Self::Error> {
        self.try_acquire_at(key, now)
    }

    fn remove_key(&self, key: &Q) -> Result<bool, Self::Error> {
        self.remove_key(key)
    }
}
