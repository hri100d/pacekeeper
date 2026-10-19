use std::time::{Duration, Instant};

use pacekeeper_core::Decision;
use pacekeeper_sliding_counter::SlidingCounterRateLimiter;

const WINDOW: Duration = Duration::from_secs(60);

#[test]
fn borrowed_keys_have_independent_bucket_histories() {
    let limiter = SlidingCounterRateLimiter::<String>::new(3, WINDOW).unwrap();

    let start = Instant::now();

    for _ in 0..3 {
        limiter.try_acquire_at("a", start).unwrap();

        limiter
            .try_acquire_at("b", start + Duration::from_secs(10))
            .unwrap();
    }

    let now = start + Duration::from_secs(80);

    assert_eq!(
        limiter.try_acquire_at("a", now).unwrap(),
        Decision::Allowed {
            remaining: 0,
            reset_after: Duration::from_secs(40),
        }
    );

    assert_eq!(
        limiter.try_acquire_at("b", now).unwrap(),
        Decision::Denied {
            retry_after: Duration::from_secs(10),
        }
    );
}

#[test]
fn removal_resets_only_the_selected_key() {
    let limiter = SlidingCounterRateLimiter::<String>::new(1, WINDOW).unwrap();

    let now = Instant::now();

    limiter.try_acquire_at("a", now).unwrap();
    limiter.try_acquire_at("b", now).unwrap();

    assert!(limiter.remove_key("a").unwrap());
    assert!(!limiter.remove_key("a").unwrap());

    assert_eq!(
        limiter.try_acquire_at("a", now).unwrap(),
        Decision::Allowed {
            remaining: 0,
            reset_after: WINDOW,
        }
    );

    assert_eq!(
        limiter.try_acquire_at("b", now).unwrap(),
        Decision::Denied {
            retry_after: Duration::from_secs(120),
        }
    );
}
