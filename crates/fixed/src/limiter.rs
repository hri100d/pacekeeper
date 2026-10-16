use crate::{errors::FixedWindowError as LimiterError, state::WindowState};

type Engine<K, T> = InMemoryWindowLimiter<K, T, WindowState<T>>;

use std::{
    borrow::Borrow,
    time::{Duration, Instant},
};

use pacekeeper_core::{
    Counter, Decision, InMemoryWindowLimiter, Key, LookupKey, RateLimiter, WindowConfig,
};

/// An in-memory fixed-window rate limiter with independent state per key.
///
/// `K` is the owned key type and `T` is the request counter type.
/// The defaults are `String` and `u64`.
///
/// Share an instance with `Arc` when multiple threads should share usage.
/// Separate instances maintain independent state.
///
/// Key entries remain stored until removed or until the limiter is dropped.
pub struct Limiter<K: Key = String, T: Counter = u64> {
    inner: Engine<K, T>,
}

impl<K: Key, T: Counter> Limiter<K, T> {
    /// Creates an empty limiter with the given limit and window.
    ///
    /// # Errors
    ///
    /// Returns a configuration error if the limit or window is zero.
    pub fn new(limit: T, window: Duration) -> Result<Self, LimiterError> {
        Ok(Self {
            inner: Engine::new(limit, window)?,
        })
    }

    /// Creates an empty limiter from a validated configuration.
    ///
    /// No per-key state is created until a key is first accessed.
    pub fn new_with_config(config: WindowConfig<T>) -> Self {
        Self {
            inner: Engine::new_with_config(config),
        }
    }

    /// Returns an independent snapshot of the current configuration.
    ///
    /// # Errors
    ///
    /// Returns an error if the internal mutex is poisoned.
    pub fn config(&self) -> Result<WindowConfig<T>, LimiterError> {
        self.inner.config()
    }

    /// Replaces the configuration while preserving existing usage.
    ///
    /// Existing counters and window starts are retained. Subsequent
    /// acquisitions evaluate them using the new limit and duration.
    ///
    /// # Errors
    ///
    /// Returns an error if the internal mutex is poisoned.
    pub fn update_config(&self, config: WindowConfig<T>) -> Result<(), LimiterError> {
        self.inner.update_config(config)
    }

    /// Attempts to acquire one request allowance using the current time.
    ///
    /// The clock is read after acquiring the internal mutex.
    /// An allowed result has already counted the request.
    ///
    /// Borrowed lookups avoid constructing an owned key for existing entries.
    ///
    /// # Errors
    ///
    /// Returns an error if the mutex is poisoned or the algorithm cannot
    /// evaluate the request.
    pub fn try_acquire<Q>(&self, key: &Q) -> Result<Decision<T>, LimiterError>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        self.inner.try_acquire(key)
    }

    /// Attempts to acquire one request allowance at an explicit timestamp.
    ///
    /// Supply nondecreasing timestamps for each key in processing order.
    /// This method is useful for deterministic tests.
    ///
    /// # Errors
    ///
    /// Returns an error if the mutex is poisoned or the algorithm cannot
    /// evaluate the request.
    pub fn try_acquire_at<Q>(&self, key: &Q, now: Instant) -> Result<Decision<T>, LimiterError>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        self.inner.try_acquire_at(key, now)
    }

    /// Removes a key and forgets its usage.
    ///
    /// Returns whether an entry existed. The next request for this key starts
    /// with fresh state.
    ///
    /// # Errors
    ///
    /// Returns an error if the internal mutex is poisoned.
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
