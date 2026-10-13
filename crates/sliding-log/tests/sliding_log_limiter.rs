use std::time::{Duration, Instant};

use pacekeeper_core::{Decision, RateLimiter, RateLimiterError, WindowConfig};
use pacekeeper_sliding_log::{SlidingLogRateLimiter, WindowStateError};

const WINDOW: Duration = Duration::from_secs(60);

#[test]
fn keys_have_independent_sliding_histories() {
    let limiter = SlidingLogRateLimiter::<String>::new(1, WINDOW).unwrap();

    let start = Instant::now();

    limiter.try_acquire_at("a", start).unwrap();
    limiter
        .try_acquire_at("b", start + Duration::from_secs(10))
        .unwrap();

    assert_eq!(
        limiter.try_acquire_at("a", start + WINDOW).unwrap(),
        Decision::Allowed {
            remaining: 0,
            reset_after: WINDOW,
        }
    );

    assert_eq!(
        limiter.try_acquire_at("b", start + WINDOW).unwrap(),
        Decision::Denied {
            retry_after: Duration::from_secs(10),
        }
    );
}

#[test]
fn limit_updates_preserve_sliding_history() {
    let limiter = SlidingLogRateLimiter::<String>::new(3, WINDOW).unwrap();

    let start = Instant::now();

    for seconds in [0, 10, 20] {
        limiter
            .try_acquire_at("a", start + Duration::from_secs(seconds))
            .unwrap();
    }

    limiter
        .update_config(WindowConfig::new(1, WINDOW).unwrap())
        .unwrap();

    assert_eq!(
        limiter.try_acquire_at("a", start + WINDOW).unwrap(),
        Decision::Denied {
            retry_after: Duration::from_secs(20),
        }
    );

    limiter
        .update_config(WindowConfig::new(3, WINDOW).unwrap())
        .unwrap();

    assert_eq!(
        limiter.try_acquire_at("a", start + WINDOW).unwrap(),
        Decision::Allowed {
            remaining: 0,
            reset_after: Duration::from_secs(10),
        }
    );
}

#[test]
fn rejected_window_updates_preserve_config_and_usage() {
    let limiter = SlidingLogRateLimiter::<String>::new(1, WINDOW).unwrap();

    let start = Instant::now();
    limiter.try_acquire_at("a", start).unwrap();

    for seconds in [30, 120] {
        assert!(matches!(
            limiter.update_config(WindowConfig::new(3, Duration::from_secs(seconds)).unwrap()),
            Err(RateLimiterError::StateError(
                WindowStateError::WindowChangeNotAllowed
            ))
        ));

        let config = limiter.config().unwrap();

        assert_eq!(config.limit(), 1);
        assert_eq!(config.window(), WINDOW);

        assert_eq!(
            limiter.try_acquire_at("a", start).unwrap(),
            Decision::Denied {
                retry_after: WINDOW,
            }
        );
    }
}
