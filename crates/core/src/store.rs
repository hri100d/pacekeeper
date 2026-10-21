use std::{
    borrow::Borrow,
    collections::HashMap,
    hash::{BuildHasher, Hash, RandomState},
    sync::Mutex,
    time::Instant,
};

use crate::{InvalidShardCount, errors::StorageError};

pub(crate) struct ShardedStorage<K, S> {
    shards: Box<[Mutex<HashMap<K, S>>]>,
    hasher: RandomState,
}

impl<K, S> ShardedStorage<K, S>
where
    K: Eq + Hash,
{
    /// Creates empty storage.
    ///
    /// Accepts shard counts 1, 2, 4, 8, ...
    /// The number of shards cannot change after construction.
    pub(crate) fn new(shard_count: usize) -> Result<Self, InvalidShardCount> {
        if !shard_count.is_power_of_two() {
            return Err(InvalidShardCount);
        }

        let shards = (0..shard_count)
            .map(|_| Mutex::new(HashMap::<K, S>::new()))
            .collect::<Vec<_>>()
            .into_boxed_slice();

        Ok(Self {
            shards,
            hasher: RandomState::new(),
        })
    }

    /// Returns the index of the shard responsible for this key.
    fn shard_index<Q>(&self, key: &Q) -> usize
    where
        Q: Hash + ?Sized,
    {
        let hash = self.hasher.hash_one(key);

        (hash as usize) & (self.shards.len() - 1)
    }

    // Lock inspection is only available to crate tests, never to consumers.
    #[cfg(test)]
    pub(crate) fn lock_for_test<Q: Hash + ?Sized>(
        &self,
        key: &Q,
    ) -> std::sync::LockResult<std::sync::MutexGuard<'_, HashMap<K, S>>> {
        self.shards[self.shard_index(key)].lock()
    }

    #[cfg(test)]
    pub(crate) fn try_lock_for_test<Q: Hash + ?Sized>(
        &self,
        key: &Q,
    ) -> std::sync::TryLockResult<std::sync::MutexGuard<'_, HashMap<K, S>>> {
        self.shards[self.shard_index(key)].try_lock()
    }

    /// Finds or creates a state and operates on it under one shard lock.
    ///
    /// The clock is sampled exactly once after acquiring the lock.
    /// The same timestamp is passed to creation and the operation.
    ///
    /// The callbacks must not re-enter this storage or perform blocking I/O.
    pub(crate) fn with_state_mut<Q, R>(
        &self,
        key: &Q,
        read_time: impl FnOnce() -> Instant,
        create: impl FnOnce(Instant) -> S,
        operation: impl FnOnce(&mut S, Instant) -> R,
    ) -> Result<R, StorageError>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ToOwned<Owned = K> + ?Sized,
    {
        let shard_index = self.shard_index(key);

        let mut shard = self.shards[shard_index]
            .lock()
            .map_err(|_| StorageError::StateUnavailable)?;

        let now = read_time();

        if let Some(state) = shard.get_mut(key) {
            return Ok(operation(state, now));
        }

        let state = shard.entry(key.to_owned()).or_insert_with(|| create(now));

        Ok(operation(state, now))
    }

    /// Removes a state and reports whether the key existed.
    pub(crate) fn remove<Q>(&self, key: &Q) -> Result<bool, StorageError>
    where
        K: Borrow<Q>,
        Q: Eq + Hash + ?Sized,
    {
        let mut shard = self.shards[self.shard_index(key)]
            .lock()
            .map_err(|_| StorageError::StateUnavailable)?;

        Ok(shard.remove(key).is_some())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{
        cell::Cell,
        panic::{AssertUnwindSafe, catch_unwind},
        sync::{
            Arc, Barrier, TryLockError,
            atomic::{AtomicUsize, Ordering},
        },
        thread,
    };

    #[test]
    fn validates_shard_counts() {
        for count in [0, 3, 6, usize::MAX] {
            assert!(matches!(
                ShardedStorage::<String, u64>::new(count),
                Err(InvalidShardCount)
            ));
        }
        for count in [1, 2, 4, 8, 16, 64] {
            assert_eq!(
                ShardedStorage::<String, u64>::new(count)
                    .unwrap()
                    .shards
                    .len(),
                count
            );
        }
    }

    #[test]
    fn samples_clock_once_under_lock_and_creates_only_missing_states() {
        let storage = ShardedStorage::<String, Instant>::new(8).unwrap();
        let now = Instant::now();
        let reads = Cell::new(0);
        let creates = Cell::new(0);
        for _ in 0..2 {
            storage
                .with_state_mut(
                    "a",
                    || {
                        assert!(matches!(
                            storage.try_lock_for_test("a"),
                            Err(TryLockError::WouldBlock)
                        ));
                        reads.set(reads.get() + 1);
                        now
                    },
                    |created_at| {
                        creates.set(creates.get() + 1);
                        created_at
                    },
                    |state, sampled| {
                        assert_eq!(*state, now);
                        assert_eq!(sampled, now);
                    },
                )
                .unwrap();
        }
        assert_eq!(reads.get(), 2);
        assert_eq!(creates.get(), 1);
        assert!(storage.remove("a").unwrap());
        assert!(!storage.remove("a").unwrap());
    }

    #[test]
    fn racing_first_acquisitions_initialize_once_and_preserve_all_updates() {
        for shards in [1, 2, 8, 64] {
            let storage = Arc::new(ShardedStorage::<u64, usize>::new(shards).unwrap());
            let barrier = Arc::new(Barrier::new(16));
            let creates = Arc::new(AtomicUsize::new(0));
            let now = Instant::now();
            let workers: Vec<_> = (0..16)
                .map(|_| {
                    let storage = Arc::clone(&storage);
                    let barrier = Arc::clone(&barrier);
                    let creates = Arc::clone(&creates);
                    thread::spawn(move || {
                        barrier.wait();
                        storage
                            .with_state_mut(
                                &42,
                                || now,
                                |_| {
                                    creates.fetch_add(1, Ordering::Relaxed);
                                    0
                                },
                                |state, _| {
                                    *state += 1;
                                    *state
                                },
                            )
                            .unwrap()
                    })
                })
                .collect();
            let mut results: Vec<_> = workers
                .into_iter()
                .map(|worker| worker.join().unwrap())
                .collect();
            results.sort_unstable();
            assert_eq!(results, (1..=16).collect::<Vec<_>>());
            assert_eq!(creates.load(Ordering::Relaxed), 1);
        }
    }

    #[test]
    fn locked_and_poisoned_shards_do_not_block_other_shards() {
        let storage = ShardedStorage::<u64, u64>::new(8).unwrap();
        let key = 0;
        let other = (1..100_000)
            .find(|other| storage.shard_index(other) != storage.shard_index(&key))
            .expect("random hashing should populate multiple shards");
        let guard = storage.lock_for_test(&key).unwrap();
        assert!(storage.try_lock_for_test(&other).is_ok());
        drop(guard);

        let panic = catch_unwind(AssertUnwindSafe(|| {
            let _ = storage.with_state_mut(
                &key,
                Instant::now,
                |_| 0,
                |_, _| panic!("poison one shard"),
            );
        }));
        assert!(panic.is_err());
        assert!(matches!(
            storage.remove(&key),
            Err(StorageError::StateUnavailable)
        ));
        assert!(matches!(
            storage.with_state_mut(&key, Instant::now, |_| 0, |_, _| ()),
            Err(StorageError::StateUnavailable)
        ));
        assert_eq!(
            storage
                .with_state_mut(&other, Instant::now, |_| 7, |state, _| *state)
                .unwrap(),
            7
        );
        assert!(storage.remove(&other).unwrap());
    }
}
