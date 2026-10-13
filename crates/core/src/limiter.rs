use std::{
    borrow::Borrow,
    collections::HashMap,
    sync::Mutex,
    time::{Duration, Instant},
};

use crate::{Counter, Decision, Key, LookupKey, RateLimiterError, State, WindowConfig};

pub trait RateLimiter<Q: ?Sized, T: Counter> {
    type Error;

    fn try_acquire(&self, key: &Q) -> Result<Decision<T>, Self::Error>;

    fn try_acquire_at(&self, key: &Q, now: Instant) -> Result<Decision<T>, Self::Error>;

    fn remove_key(&self, key: &Q) -> Result<bool, Self::Error>;
}

pub struct InMemoryWindowLimiter<K: Key, T: Counter, S: State<T>> {
    inner: Mutex<Inner<K, T, S>>,
}

struct Inner<K, T: Counter, S> {
    config: WindowConfig<T>,
    windows: HashMap<K, S>,
}

impl<K, T, S> InMemoryWindowLimiter<K, T, S>
where
    K: Key,
    T: Counter,
    S: State<T>,
{
    pub fn new(limit: T, window: Duration) -> Result<Self, RateLimiterError<S::Error>> {
        Ok(Self::new_with_config(WindowConfig::new(limit, window)?))
    }

    pub fn new_with_config(config: WindowConfig<T>) -> Self {
        Self {
            inner: Mutex::new(Inner {
                config,
                windows: HashMap::new(),
            }),
        }
    }

    fn acquire_with_clock<Q>(
        &self,
        key: &Q,
        read_time: impl FnOnce() -> Instant,
    ) -> Result<Decision<T>, RateLimiterError<S::Error>>
    where
        K: Borrow<Q>,
        Q: LookupKey<K> + ?Sized,
    {
        let mut inner = self.inner.lock()?;
        let now = read_time();

        let limit = inner.config.limit();
        let window = inner.config.window();

        if let Some(state) = inner.windows.get_mut(key) {
            return state
                .try_accept(now, limit, window)
                .map_err(RateLimiterError::StateError);
        }

        let state = inner
            .windows
            .entry(key.to_owned())
            .or_insert_with(|| S::new(now));

        state
            .try_accept(now, limit, window)
            .map_err(RateLimiterError::StateError)
    }

    pub fn config(&self) -> Result<WindowConfig<T>, RateLimiterError<S::Error>> {
        Ok(self.inner.lock()?.config.clone())
    }

    pub fn update_config(&self, config: WindowConfig<T>) -> Result<(), RateLimiterError<S::Error>> {
        let mut inner = self.inner.lock()?;

        S::validate_config_change(&inner.config, &config).map_err(RateLimiterError::StateError)?;

        inner.config = config;

        Ok(())
    }
}

impl<K, Q, T, S> RateLimiter<Q, T> for InMemoryWindowLimiter<K, T, S>
where
    K: Key + Borrow<Q>,
    Q: LookupKey<K> + ?Sized,
    T: Counter,
    S: State<T>,
{
    type Error = RateLimiterError<S::Error>;

    fn try_acquire(&self, key: &Q) -> Result<Decision<T>, Self::Error> {
        self.acquire_with_clock(key, Instant::now)
    }

    fn try_acquire_at(&self, key: &Q, now: Instant) -> Result<Decision<T>, Self::Error> {
        self.acquire_with_clock(key, || now)
    }

    fn remove_key(&self, key: &Q) -> Result<bool, Self::Error> {
        Ok(self.inner.lock()?.windows.remove(key).is_some())
    }
}

#[cfg(test)]
mod limiter_tests {
    use super::*;
    use std::panic::{AssertUnwindSafe, catch_unwind};
    use std::sync::TryLockError;

    #[derive(Debug, PartialEq, Eq)]
    enum TestError {
        Rejected,
    }

    struct RecordingState {
        created_at: Instant,
        calls: usize,
        last_input: Option<(Instant, u64, Duration)>,
    }

    impl State<u64> for RecordingState {
        type Error = TestError;

        fn new(now: Instant) -> Self {
            Self {
                created_at: now,
                calls: 0,
                last_input: None,
            }
        }

        fn try_accept(
            &mut self,
            now: Instant,
            limit: u64,
            window: Duration,
        ) -> Result<Decision<u64>, Self::Error> {
            self.calls += 1;
            self.last_input = Some((now, limit, window));

            Ok(Decision::Allowed {
                remaining: limit - 1,
                reset_after: window,
            })
        }
    }

    struct FailingState;

    impl State<u64> for FailingState {
        type Error = TestError;

        fn new(_: Instant) -> Self {
            Self
        }

        fn try_accept(
            &mut self,
            _: Instant,
            _: u64,
            _: Duration,
        ) -> Result<Decision<u64>, Self::Error> {
            Err(TestError::Rejected)
        }
    }

    type Limiter = InMemoryWindowLimiter<String, u64, RecordingState>;

    #[test]
    fn constructor_wraps_invalid_configuration() {
        assert!(matches!(
            Limiter::new(0, Duration::from_secs(60)),
            Err(RateLimiterError::ConfigError(crate::ConfigError::ZeroLimit))
        ));

        assert!(matches!(
            Limiter::new(1, Duration::ZERO),
            Err(RateLimiterError::ConfigError(
                crate::ConfigError::ZeroWindow
            ))
        ));
    }

    #[test]
    fn config_returns_an_independent_snapshot() {
        let window = Duration::from_secs(60);
        let limiter = Limiter::new_with_config(WindowConfig::new(3, window).unwrap());

        let snapshot = limiter.config().unwrap();

        limiter
            .update_config(WindowConfig::new(5, window).unwrap())
            .unwrap();

        assert_eq!(snapshot.limit(), 3);
        assert_eq!(snapshot.window(), window);
        assert_eq!(limiter.config().unwrap().limit(), 5);
    }

    #[test]
    fn forwards_inputs_and_preserves_state_across_config_updates() {
        let window = Duration::from_secs(60);
        let limiter = Limiter::new(3, window).unwrap();
        let start = Instant::now();

        assert_eq!(
            limiter.try_acquire_at("a", start).unwrap(),
            Decision::Allowed {
                remaining: 2,
                reset_after: window,
            }
        );

        limiter
            .update_config(WindowConfig::new(5, window).unwrap())
            .unwrap();

        let later = start + Duration::from_secs(10);

        assert_eq!(
            limiter.try_acquire_at("a", later).unwrap(),
            Decision::Allowed {
                remaining: 4,
                reset_after: window,
            }
        );

        let inner = limiter.inner.lock().unwrap();
        let state = inner.windows.get("a").unwrap();

        assert_eq!(state.created_at, start);
        assert_eq!(state.calls, 2);
        assert_eq!(state.last_input, Some((later, 5, window)));
    }

    #[test]
    fn reads_clock_after_acquiring_the_mutex() {
        let limiter = Limiter::new(3, Duration::from_secs(60)).unwrap();
        let now = Instant::now();
        let mut reads = 0;

        limiter
            .acquire_with_clock("a", || {
                reads += 1;

                assert!(matches!(
                    limiter.inner.try_lock(),
                    Err(TryLockError::WouldBlock)
                ));

                now
            })
            .unwrap();

        assert_eq!(reads, 1);

        let inner = limiter.inner.lock().unwrap();
        assert_eq!(inner.windows.get("a").unwrap().created_at, now);
    }

    #[test]
    fn wraps_algorithm_errors_on_insertion_and_existing_lookup() {
        let limiter =
            InMemoryWindowLimiter::<String, u64, FailingState>::new(1, Duration::from_secs(60))
                .unwrap();

        let now = Instant::now();

        for _ in 0..2 {
            assert!(matches!(
                limiter.try_acquire_at("a", now),
                Err(RateLimiterError::StateError(TestError::Rejected))
            ));
        }
    }

    #[test]
    fn supports_calls_through_the_rate_limiter_trait() {
        let limiter = Limiter::new(3, Duration::from_secs(60)).unwrap();

        let limiter: &dyn RateLimiter<str, u64, Error = RateLimiterError<TestError>> = &limiter;

        assert!(matches!(
            limiter.try_acquire("a").unwrap(),
            Decision::Allowed { remaining: 2, .. }
        ));

        assert!(matches!(
            limiter.try_acquire_at("b", Instant::now()).unwrap(),
            Decision::Allowed { remaining: 2, .. }
        ));

        assert!(limiter.remove_key("a").unwrap());
        assert!(!limiter.remove_key("a").unwrap());
    }

    #[test]
    fn poisoned_mutex_returns_errors_from_all_operations() {
        let window = Duration::from_secs(60);
        let limiter = Limiter::new(3, window).unwrap();

        let panic = catch_unwind(AssertUnwindSafe(|| {
            let _guard = limiter.inner.lock().unwrap();
            panic!("deliberately poison the mutex");
        }));

        assert!(panic.is_err());

        assert!(matches!(
            limiter.try_acquire("a"),
            Err(RateLimiterError::StateUnavailable)
        ));

        assert!(matches!(
            limiter.try_acquire_at("a", Instant::now()),
            Err(RateLimiterError::StateUnavailable)
        ));

        assert!(matches!(
            limiter.remove_key("a"),
            Err(RateLimiterError::StateUnavailable)
        ));

        assert!(matches!(
            limiter.config(),
            Err(RateLimiterError::StateUnavailable)
        ));

        assert!(matches!(
            limiter.update_config(WindowConfig::new(5, window).unwrap()),
            Err(RateLimiterError::StateUnavailable)
        ));
    }
}
