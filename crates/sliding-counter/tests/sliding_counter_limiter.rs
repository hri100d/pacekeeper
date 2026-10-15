use std::time::{Duration, Instant};

use pacekeeper_core::{Decision, RateLimiterError, WindowConfig};
use pacekeeper_sliding_counter::{SlidingCounterRateLimiter, WindowStateError};

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

#[test]
fn limit_updates_preserve_existing_usage() {
    let limiter = SlidingCounterRateLimiter::<String>::new(3, WINDOW).unwrap();

    let start = Instant::now();

    for _ in 0..3 {
        limiter.try_acquire_at("a", start).unwrap();
    }

    limiter
        .update_config(WindowConfig::new(1, WINDOW).unwrap())
        .unwrap();

    assert_eq!(
        limiter.try_acquire_at("a", start + WINDOW).unwrap(),
        Decision::Denied {
            retry_after: WINDOW,
        }
    );

    limiter
        .update_config(WindowConfig::new(4, WINDOW).unwrap())
        .unwrap();

    assert_eq!(
        limiter.try_acquire_at("a", start + WINDOW).unwrap(),
        Decision::Allowed {
            remaining: 0,
            reset_after: WINDOW,
        }
    );
}

#[test]
fn rejected_duration_changes_preserve_configuration_and_usage() {
    let limiter = SlidingCounterRateLimiter::<String>::new(1, WINDOW).unwrap();

    let now = Instant::now();
    limiter.try_acquire_at("a", now).unwrap();

    for seconds in [30, 120] {
        assert!(matches!(
            limiter.update_config(WindowConfig::new(3, Duration::from_secs(seconds),).unwrap()),
            Err(RateLimiterError::StateError(
                WindowStateError::WindowChangeNotAllowed
            ))
        ));

        let config = limiter.config().unwrap();

        assert_eq!(config.limit(), 1);
        assert_eq!(config.window(), WINDOW);

        assert_eq!(
            limiter.try_acquire_at("a", now).unwrap(),
            Decision::Denied {
                retry_after: Duration::from_secs(120),
            }
        );
    }
}
