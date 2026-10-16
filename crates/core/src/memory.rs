use std::{borrow::Borrow, collections::HashMap, sync::Mutex, time::Instant};

use crate::{Decision, InMemoryError, InMemoryState, Key, LookupKey};

/// A mutex-protected map of per-key algorithm states.
///
/// Intended for implementing synchronous in-memory limiters.
/// `S` defines state initialization, acquisition, and configuration-change
/// validation through `InMemoryState`.
///
/// All keys share one mutex. Existing-key lookups avoid creating another
/// owned key. Entries are retained until explicitly removed.
///
/// Configuration updates preserve existing states.
pub struct InMemoryLimiter<K, Config, S> {
    pub(crate) inner: Mutex<Inner<K, Config, S>>,
}

pub(crate) struct Inner<K, Config, S> {
    pub(crate) config: Config,
    pub(crate) windows: HashMap<K, S>,
}

impl<K, Config, S> InMemoryLimiter<K, Config, S>
where
    K: Key,
    S: InMemoryState<Config>,
{
    pub fn new_with_config(config: Config) -> Self {
        Self {
            inner: Mutex::new(Inner {
                config,
                windows: HashMap::new(),
            }),
        }
    }

    pub fn config(&self) -> Result<Config, InMemoryError<S::Error>>
    where
        Config: Clone,
    {
        let inner = self
            .inner
            .lock()
            .map_err(|_| InMemoryError::StateUnavailable)?;

        Ok(inner.config.clone())
    }

    pub fn update_config(&self, config: Config) -> Result<(), InMemoryError<S::Error>> {
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| InMemoryError::StateUnavailable)?;

        S::validate_config_change(&inner.config, &config).map_err(InMemoryError::StateError)?;

        inner.config = config;

        Ok(())
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
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| InMemoryError::StateUnavailable)?;

        Ok(inner.windows.remove(key).is_some())
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
        let mut inner = self
            .inner
            .lock()
            .map_err(|_| InMemoryError::StateUnavailable)?;

        let now = read_time();

        let Inner { config, windows } = &mut *inner;

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
