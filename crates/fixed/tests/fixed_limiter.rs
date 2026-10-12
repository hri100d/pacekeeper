use std::hash::{Hash, Hasher};
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::{Arc, Barrier};
use std::thread;
use std::time::{Duration, Instant};

use pacekeeper_core::Decision;
use pacekeeper_fixed::{FixedRateLimiter, FixedWindowConfig};

#[test]
fn supports_borrowed_string_keys() {
    let window = Duration::from_secs(60);
    let limiter = FixedRateLimiter::<String>::new(1, window).unwrap();
    let now = Instant::now();
    let first = String::from("user:123");
    let second = String::from("user:123");

    assert_eq!(
        limiter.try_acquire_at(first.as_str(), now).unwrap(),
        Decision::Allowed {
            remaining: 0,
            reset_after: window,
        }
    );
    assert_eq!(
        limiter.try_acquire_at(second.as_str(), now).unwrap(),
        Decision::Denied {
            retry_after: window
        }
    );
}

#[test]
fn keeps_numeric_keys_independent() {
    let window = Duration::from_secs(60);
    let limiter = FixedRateLimiter::<u64>::new(1, window).unwrap();
    let now = Instant::now();

    assert!(matches!(
        limiter.try_acquire_at(&42, now).unwrap(),
        Decision::Allowed { remaining: 0, .. }
    ));
    assert!(matches!(
        limiter.try_acquire_at(&42, now).unwrap(),
        Decision::Denied { .. }
    ));
    assert_eq!(
        limiter.try_acquire_at(&43, now).unwrap(),
        Decision::Allowed {
            remaining: 0,
            reset_after: window,
        }
    );
}

#[test]
fn supports_composite_keys() {
    #[derive(Clone, PartialEq, Eq, Hash)]
    struct UserKey {
        organization_id: u64,
        user_id: u64,
    }

    let window = Duration::from_secs(60);
    let limiter = FixedRateLimiter::<UserKey>::new(1, window).unwrap();
    let now = Instant::now();
    let key = UserKey {
        organization_id: 1,
        user_id: 10,
    };
    let equal_key = UserKey {
        organization_id: 1,
        user_id: 10,
    };

    assert!(matches!(
        limiter.try_acquire_at(&key, now).unwrap(),
        Decision::Allowed { .. }
    ));
    assert!(matches!(
        limiter.try_acquire_at(&equal_key, now).unwrap(),
        Decision::Denied { .. }
    ));

    for other in [
        UserKey {
            organization_id: 2,
            user_id: 10,
        },
        UserKey {
            organization_id: 1,
            user_id: 11,
        },
    ] {
        assert!(matches!(
            limiter.try_acquire_at(&other, now).unwrap(),
            Decision::Allowed { .. }
        ));
    }
}

#[test]
fn copies_keys_only_on_first_insertion() {
    struct TrackedKey {
        id: u64,
        copies: Arc<AtomicUsize>,
    }

    impl Clone for TrackedKey {
        fn clone(&self) -> Self {
            self.copies.fetch_add(1, Ordering::Relaxed);
            Self {
                id: self.id,
                copies: Arc::clone(&self.copies),
            }
        }
    }

    impl PartialEq for TrackedKey {
        fn eq(&self, other: &Self) -> bool {
            self.id == other.id
        }
    }
    impl Eq for TrackedKey {}

    impl Hash for TrackedKey {
        fn hash<H: Hasher>(&self, state: &mut H) {
            self.id.hash(state);
        }
    }

    let copies = Arc::new(AtomicUsize::new(0));
    let key = TrackedKey {
        id: 42,
        copies: Arc::clone(&copies),
    };
    let limiter = FixedRateLimiter::<TrackedKey>::new(1, Duration::from_secs(60)).unwrap();
    let now = Instant::now();

    assert!(matches!(
        limiter.try_acquire_at(&key, now).unwrap(),
        Decision::Allowed { .. }
    ));
    assert_eq!(copies.load(Ordering::Relaxed), 1);

    for _ in 0..3 {
        assert!(matches!(
            limiter.try_acquire_at(&key, now).unwrap(),
            Decision::Denied { .. }
        ));
    }

    assert_eq!(copies.load(Ordering::Relaxed), 1);
    assert!(limiter.remove_key(&key).unwrap());
    assert_eq!(copies.load(Ordering::Relaxed), 1);
}

#[test]
fn remove_key_clears_only_that_keys_usage() {
    let window = Duration::from_secs(60);
    let limiter = FixedRateLimiter::<String>::new(1, window).unwrap();
    let now = Instant::now();

    for key in ["a", "b"] {
        assert!(matches!(
            limiter.try_acquire_at(key, now).unwrap(),
            Decision::Allowed { .. }
        ));
    }

    assert!(limiter.remove_key("a").unwrap());
    assert!(!limiter.remove_key("a").unwrap());
    assert_eq!(
        limiter.try_acquire_at("a", now).unwrap(),
        Decision::Allowed {
            remaining: 0,
            reset_after: window,
        }
    );
    assert_eq!(
        limiter.try_acquire_at("b", now).unwrap(),
        Decision::Denied {
            retry_after: window
        }
    );
}

#[test]
fn increasing_limit_preserves_usage() {
    let window = Duration::from_secs(60);
    let limiter = FixedRateLimiter::<String>::new(3, window).unwrap();
    let now = Instant::now();

    for _ in 0..2 {
        assert!(matches!(
            limiter.try_acquire_at("a", now).unwrap(),
            Decision::Allowed { .. }
        ));
    }

    limiter
        .update_config(FixedWindowConfig::new(5, window).unwrap())
        .unwrap();

    assert_eq!(limiter.config().unwrap().limit(), 5);

    for remaining in [2, 1, 0] {
        assert_eq!(
            limiter.try_acquire_at("a", now).unwrap(),
            Decision::Allowed {
                remaining,
                reset_after: window,
            }
        );
    }

    assert_eq!(
        limiter.try_acquire_at("a", now).unwrap(),
        Decision::Denied {
            retry_after: window
        }
    );
}

#[test]
fn decreasing_limit_preserves_usage() {
    let window = Duration::from_secs(60);
    let limiter = FixedRateLimiter::<String>::new(5, window).unwrap();
    let start = Instant::now();

    for _ in 0..3 {
        assert!(matches!(
            limiter.try_acquire_at("a", start).unwrap(),
            Decision::Allowed { .. }
        ));
    }

    limiter
        .update_config(FixedWindowConfig::new(2, window).unwrap())
        .unwrap();

    assert_eq!(
        limiter.try_acquire_at("a", start).unwrap(),
        Decision::Denied {
            retry_after: window
        }
    );
    assert_eq!(
        limiter.try_acquire_at("a", start + window).unwrap(),
        Decision::Allowed {
            remaining: 1,
            reset_after: window,
        }
    );
}

#[test]
fn extending_window_preserves_start_and_usage() {
    let original = Duration::from_secs(60);
    let extended = Duration::from_secs(120);
    let limiter = FixedRateLimiter::<String>::new(1, original).unwrap();
    let start = Instant::now();

    assert!(matches!(
        limiter.try_acquire_at("a", start).unwrap(),
        Decision::Allowed { .. }
    ));

    limiter
        .update_config(FixedWindowConfig::new(1, extended).unwrap())
        .unwrap();

    assert_eq!(limiter.config().unwrap().window(), extended);
    assert_eq!(
        limiter
            .try_acquire_at("a", start + Duration::from_secs(70))
            .unwrap(),
        Decision::Denied {
            retry_after: Duration::from_secs(50)
        }
    );
    assert_eq!(
        limiter.try_acquire_at("a", start + extended).unwrap(),
        Decision::Allowed {
            remaining: 0,
            reset_after: extended,
        }
    );
}

#[test]
fn shortening_window_can_expire_existing_state() {
    let limiter = FixedRateLimiter::<String>::new(1, Duration::from_secs(60)).unwrap();
    let start = Instant::now();

    assert!(matches!(
        limiter.try_acquire_at("a", start).unwrap(),
        Decision::Allowed { .. }
    ));

    let shortened = Duration::from_secs(30);

    limiter
        .update_config(FixedWindowConfig::new(1, shortened).unwrap())
        .unwrap();

    let now = start + Duration::from_secs(40);

    assert_eq!(
        limiter.try_acquire_at("a", now).unwrap(),
        Decision::Allowed {
            remaining: 0,
            reset_after: shortened,
        }
    );
    assert_eq!(
        limiter.try_acquire_at("a", now).unwrap(),
        Decision::Denied {
            retry_after: shortened
        }
    );
}

#[test]
fn shared_limiter_enforces_limit_under_concurrency() {
    let window = Duration::from_secs(60);
    let limiter = Arc::new(FixedRateLimiter::<u64>::new(5, window).unwrap());
    let worker_count = 32;
    let barrier = Arc::new(Barrier::new(worker_count + 1));
    let now = Instant::now();

    let workers: Vec<_> = (0..worker_count)
        .map(|_| {
            let limiter = Arc::clone(&limiter);
            let barrier = Arc::clone(&barrier);
            thread::spawn(move || {
                barrier.wait();
                limiter.try_acquire_at(&42, now).unwrap()
            })
        })
        .collect();

    barrier.wait();
    let mut allowed = 0;
    let mut denied = 0;

    for worker in workers {
        match worker.join().expect("worker must not panic") {
            Decision::Allowed {
                remaining,
                reset_after,
            } => {
                allowed += 1;
                assert!(remaining < 5);
                assert_eq!(reset_after, window);
            }
            Decision::Denied { retry_after } => {
                denied += 1;
                assert_eq!(retry_after, window);
            }
        }
    }

    assert_eq!(allowed, 5);
    assert_eq!(denied, 27);
}

#[test]
fn automatic_clock_supports_normal_acquisition() {
    let window = Duration::from_secs(60);
    let config = FixedWindowConfig::new(3u64, window).unwrap();
    let limiter = FixedRateLimiter::<String>::new_with_config(config);

    match limiter.try_acquire("a").unwrap() {
        Decision::Allowed {
            remaining,
            reset_after,
        } => {
            assert_eq!(remaining, 2);
            assert!(reset_after > Duration::ZERO);
            assert!(reset_after <= window);
        }
        Decision::Denied { .. } => panic!("the first request must be allowed"),
    }
}
