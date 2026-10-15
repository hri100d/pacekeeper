use crate::{errors::SlidingCounterWindowError as LimiterError, state::WindowState};

type Engine<K, T> = InMemoryWindowLimiter<K, T, WindowState<T>>;

use std::{
    borrow::Borrow,
    time::{Duration, Instant},
};

use pacekeeper_core::{
    Counter, Decision, InMemoryWindowLimiter, Key, LookupKey, RateLimiter, WindowConfig,
};

pub struct Limiter<K: Key = String, T: Counter = u64> {
    inner: Engine<K, T>,
}

impl<K: Key, T: Counter> Limiter<K, T> {
    pub fn new(limit: T, window: Duration) -> Result<Self, LimiterError> {
        Ok(Self {
            inner: Engine::new(limit, window)?,
        })
    }

    pub fn new_with_config(config: WindowConfig<T>) -> Self {
        Self {
            inner: Engine::new_with_config(config),
        }
    }

    pub fn config(&self) -> Result<WindowConfig<T>, LimiterError> {
        self.inner.config()
    }

    pub fn update_config(&self, config: WindowConfig<T>) -> Result<(), LimiterError> {
        self.inner.update_config(config)
    }

    pub fn try_acquire<Q>(&self, key: &Q) -> Result<Decision<T>, LimiterError>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        self.inner.try_acquire(key)
    }

    pub fn try_acquire_at<Q>(&self, key: &Q, now: Instant) -> Result<Decision<T>, LimiterError>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        self.inner.try_acquire_at(key, now)
    }

    pub fn remove_key<Q>(&self, key: &Q) -> Result<bool, LimiterError>
    where
        K: Borrow<Q>,
        Q: Key + ?Sized,
    {
        self.inner.remove_key(key)
    }
}

impl<K, Q, T> RateLimiter<Q, T> for Limiter<K, T>
where
    K: Key + Borrow<Q>,
    Q: LookupKey<K> + ?Sized,
    T: Counter,
{
    type Error = LimiterError;

    fn try_acquire(&self, key: &Q) -> Result<Decision<T>, Self::Error> {
        Limiter::try_acquire(self, key)
    }

    fn try_acquire_at(&self, key: &Q, now: Instant) -> Result<Decision<T>, Self::Error> {
        Limiter::try_acquire_at(self, key, now)
    }

    fn remove_key(&self, key: &Q) -> Result<bool, Self::Error> {
        Limiter::remove_key(self, key)
    }
}
