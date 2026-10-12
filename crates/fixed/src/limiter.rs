use std::borrow::Borrow;
use std::collections::HashMap;
use std::sync::Mutex;
use std::time::{Duration, Instant};

use pacekeeper_core::{Counter, Decision, Key, LookupKey};

use crate::FixedWindowError;
use crate::config::FixedWindowConfig;
use crate::state::WindowStats;

pub struct FixedRateLimiter<K: Key = String, T: Counter = u64> {
    inner: Mutex<Inner<K, T>>,
}

struct Inner<K, T: Counter> {
    config: FixedWindowConfig<T>,
    windows: HashMap<K, WindowStats<T>>,
}

impl<K: Key, T: Counter> FixedRateLimiter<K, T> {
    pub fn new(limit: T, window: Duration) -> Result<Self, FixedWindowError> {
        Ok(Self::new_with_config(FixedWindowConfig::new(
            limit, window,
        )?))
    }

    pub fn new_with_config(config: FixedWindowConfig<T>) -> Self {
        Self {
            inner: Mutex::new(Inner {
                config,
                windows: HashMap::new(),
            }),
        }
    }

    pub fn try_acquire<Q>(&self, key: &Q) -> Result<Decision<T>, FixedWindowError>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        self.acquire_with_clock(key, Instant::now)
    }

    pub fn try_acquire_at<Q>(&self, key: &Q, now: Instant) -> Result<Decision<T>, FixedWindowError>
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
    ) -> Result<Decision<T>, FixedWindowError>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        let mut inner = self.inner.lock()?;
        let now = read_time();

        let limit = inner.config.limit();
        let window = inner.config.window();

        if let Some(entry) = inner.windows.get_mut(key) {
            return Ok(entry.try_accept(now, limit, window)?);
        }

        let entry = inner
            .windows
            .entry(key.to_owned())
            .or_insert_with(|| WindowStats::new(now));

        Ok(entry.try_accept(now, limit, window)?)
    }

    pub fn remove_key<Q>(&self, key: &Q) -> Result<bool, FixedWindowError>
    where
        K: Borrow<Q>,
        Q: Key + ?Sized,
    {
        Ok(self.inner.lock()?.windows.remove(key).is_some())
    }

    pub fn config(&self) -> Result<FixedWindowConfig<T>, FixedWindowError> {
        Ok(self.inner.lock()?.config.clone())
    }

    pub fn update_config(&self, config: FixedWindowConfig<T>) -> Result<(), FixedWindowError> {
        self.inner.lock()?.config = config;

        Ok(())
    }
}
