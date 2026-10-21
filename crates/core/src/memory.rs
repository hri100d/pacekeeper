use std::{borrow::Borrow, time::Instant};

use crate::{
    Decision, InMemoryError, InMemoryState, InvalidShardCount, Key, LookupKey,
    store::ShardedStorage,
};

/// Sharded, mutex-protected storage of per-key algorithm states.
///
/// Intended for implementing synchronous in-memory limiters.
/// `S` defines state initialization and acquisition through `InMemoryState`.
///
/// Defaults to one shard. With multiple shards, keys on different shards can
/// be processed concurrently. Existing-key lookups avoid creating another
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
    pub(crate) windows: ShardedStorage<K, S>,
}

impl<K, Config, S> InMemoryLimiter<K, Config, S>
where
    K: Key,
    S: InMemoryState<Config>,
{
    /// Creates an empty limiter with a fixed configuration and one shard.
    pub fn new_with_config(config: Config) -> Self {
        Self::new_with_config_and_shards(config, 1).expect("one is a valid shard count")
    }

    /// Creates an empty limiter with a fixed configuration and shard count.
    ///
    /// The shard count is fixed for the lifetime of the limiter. A poisoned
    /// shard prevents state operations on all keys routed to that shard;
    /// other shards and configuration reads remain available.
    ///
    /// # Errors
    ///
    /// Returns [`InvalidShardCount`] for zero or a non-power-of-two count.
    pub fn new_with_config_and_shards(
        config: Config,
        shard_count: usize,
    ) -> Result<Self, InvalidShardCount> {
        Ok(Self {
            inner: Inner {
                config,
                windows: ShardedStorage::new(shard_count)?,
            },
        })
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
        self.inner.windows.remove(key).map_err(InMemoryError::from)
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
        let config = &self.inner.config;
        self.inner
            .windows
            .with_state_mut(
                key,
                read_time,
                |now| S::new(config, now),
                |state, now| state.try_accept(now, config),
            )
            .map_err(InMemoryError::from)?
            .map_err(InMemoryError::StateError)
    }
}
