#![cfg(feature = "memory")]

use std::time::{Duration, Instant};

use pacekeeper_bucket::{BucketError, Decision, TokenBucket};
use pacekeeper_core::RateLimiter;

#[test]
fn starts_full_consumes_once_and_works_through_the_trait() {
    let limiter = TokenBucket::<String, u8, u16>::new(3u8, 1u16, Duration::from_secs(1)).unwrap();

    let limiter: &dyn RateLimiter<str, u8, Error = BucketError> = &limiter;
    let now = Instant::now();

    for remaining in (0u8..3).rev() {
        assert_eq!(
            limiter.try_acquire_at("user", now).unwrap(),
            Decision::Allowed {
                remaining,
                reset_after: Duration::from_secs(u64::from(3 - remaining),),
            },
        );
    }

    assert_eq!(
        limiter.try_acquire_at("user", now).unwrap(),
        Decision::Denied {
            retry_after: Duration::from_secs(1),
        },
    );
}

#[test]
fn denied_requests_preserve_fractional_refill_progress() {
    let limiter = TokenBucket::<String>::new(1, 1, Duration::from_secs(1)).unwrap();

    let start = Instant::now();

    assert!(matches!(
        limiter.try_acquire_at("user", start).unwrap(),
        Decision::Allowed { remaining: 0, .. }
    ));

    for elapsed_us in [250_000u64, 500_000, 999_999] {
        assert_eq!(
            limiter
                .try_acquire_at("user", start + Duration::from_micros(elapsed_us),)
                .unwrap(),
            Decision::Denied {
                retry_after: Duration::from_micros(1_000_000 - elapsed_us,),
            },
        );
    }

    assert_eq!(
        limiter
            .try_acquire_at("user", start + Duration::from_secs(1))
            .unwrap(),
        Decision::Allowed {
            remaining: 0,
            reset_after: Duration::from_secs(1),
        },
    );
}

#[test]
fn preserves_submicrosecond_elapsed_time_between_requests() {
    let limiter = TokenBucket::<String>::new(1, 1, Duration::from_micros(2)).unwrap();

    let start = Instant::now();

    limiter.try_acquire_at("user", start).unwrap();

    assert_eq!(
        limiter
            .try_acquire_at("user", start + Duration::from_nanos(1_500),)
            .unwrap(),
        Decision::Denied {
            retry_after: Duration::from_micros(1),
        },
    );

    assert_eq!(
        limiter
            .try_acquire_at("user", start + Duration::from_nanos(2_000),)
            .unwrap(),
        Decision::Allowed {
            remaining: 0,
            reset_after: Duration::from_micros(2),
        },
    );
}

#[test]
fn long_idle_period_does_not_accumulate_above_capacity() {
    let limiter = TokenBucket::<String>::new(3, 1, Duration::from_secs(1)).unwrap();

    let start = Instant::now();

    for _ in 0..3 {
        limiter.try_acquire_at("user", start).unwrap();
    }

    let later = start + Duration::from_secs(3_600);

    for remaining in (0u64..3).rev() {
        assert_eq!(
            limiter.try_acquire_at("user", later).unwrap(),
            Decision::Allowed {
                remaining,
                reset_after: Duration::from_secs(3 - remaining),
            },
        );
    }

    assert_eq!(
        limiter.try_acquire_at("user", later).unwrap(),
        Decision::Denied {
            retry_after: Duration::from_secs(1),
        },
    );
}

#[test]
fn backwards_time_returns_an_error_without_changing_usage() {
    let limiter = TokenBucket::<String>::new(2, 1, Duration::from_secs(1)).unwrap();

    let start = Instant::now();
    let later = start + Duration::from_millis(500);

    limiter.try_acquire_at("user", start).unwrap();
    limiter.try_acquire_at("user", later).unwrap();

    assert!(matches!(
        limiter.try_acquire_at("user", start),
        Err(BucketError::ClockWentBackwards)
    ));

    assert_eq!(
        limiter.try_acquire_at("user", later).unwrap(),
        Decision::Denied {
            retry_after: Duration::from_millis(500),
        },
    );
}

#[test]
fn removing_one_key_preserves_other_keys_usage() {
    let period = Duration::from_secs(60);
    let limiter = TokenBucket::<String>::new(1, 1, period).unwrap();
    let now = Instant::now();

    for key in ["a", "b"] {
        limiter.try_acquire_at(key, now).unwrap();
    }

    assert!(limiter.remove_key("a").unwrap());
    assert!(!limiter.remove_key("a").unwrap());

    assert_eq!(
        limiter.try_acquire_at("a", now).unwrap(),
        Decision::Allowed {
            remaining: 0,
            reset_after: period,
        },
    );

    assert_eq!(
        limiter.try_acquire_at("b", now).unwrap(),
        Decision::Denied {
            retry_after: period,
        },
    );
}
