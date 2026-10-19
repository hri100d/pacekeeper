use std::{borrow::Borrow, collections::HashMap, sync::Mutex, time::Instant};

use crate::{Decision, InMemoryError, InMemoryState, Key, LookupKey};

/// A mutex-protected map of per-key algorithm states.
///
/// Intended for implementing synchronous in-memory limiters.
/// `S` defines state initialization and acquisition through `InMemoryState`.
///
/// All keys share one mutex. Existing-key lookups avoid creating another
/// owned key. Entries are retained until explicitly removed.
///
/// Configuration is fixed at construction and stored outside the state mutex.
/// The engine owns `Config` directly and borrows it during acquisition.
/// Reading it does not acquire the state lock. Share the entire limiter with
/// `Arc` when threads should share both configuration and usage.
pub struct InMemoryLimiter<K, Config, S> {
    pub(crate) inner: Inner<K, Config, S>,
}

pub(crate) struct Inner<K, Config, S> {
    pub(crate) config: Config,
    pub(crate) windows: Mutex<HashMap<K, S>>,
}

impl<K, Config, S> InMemoryLimiter<K, Config, S>
where
    K: Key,
    S: InMemoryState<Config>,
{
    /// Creates an empty limiter with a fixed configuration.
    pub fn new_with_config(config: Config) -> Self {
        Self {
            inner: Inner {
                config,
                windows: Mutex::new(HashMap::new()),
            },
        }
    }

    /// Clones the configuration without acquiring the state mutex.
    ///
    /// This remains available if the state mutex is poisoned.
    /// Only this accessor requires `Config: Clone`.
    pub fn config(&self) -> Config
    where
        Config: Clone,
    {
        self.inner.config.clone()
    }

    pub fn try_acquire<Q>(&self, key: &Q) -> Result<Decision<S::Counter>, InMemoryError<S::Error>>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        self.acquire_with_clock(key, Instant::now)
    }

    pub fn try_acquire_at<Q>(
        &self,
        key: &Q,
        now: Instant,
    ) -> Result<Decision<S::Counter>, InMemoryError<S::Error>>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        self.acquire_with_clock(key, || now)
    }

    pub fn remove_key<Q>(&self, key: &Q) -> Result<bool, InMemoryError<S::Error>>
    where
        K: Borrow<Q>,
        Q: Key + ?Sized,
    {
        let mut windows = self
            .inner
            .windows
            .lock()
            .map_err(|_| InMemoryError::StateUnavailable)?;

        Ok(windows.remove(key).is_some())
    }

    pub(crate) fn acquire_with_clock<Q>(
        &self,
        key: &Q,
        read_time: impl FnOnce() -> Instant,
    ) -> Result<Decision<S::Counter>, InMemoryError<S::Error>>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        let mut windows = self
            .inner
            .windows
            .lock()
            .map_err(|_| InMemoryError::StateUnavailable)?;

        let config = &self.inner.config;

        let now = read_time();

        if let Some(state) = windows.get_mut(key) {
            return state
                .try_accept(now, config)
                .map_err(InMemoryError::StateError);
        }

        let state = windows
            .entry(key.to_owned())
            .or_insert_with(|| S::new(config, now));

        state
            .try_accept(now, config)
            .map_err(InMemoryError::StateError)
    }
}
