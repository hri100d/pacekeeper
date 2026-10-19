use std::time::{Duration, Instant};

use pacekeeper_core::Decision;
use pacekeeper_sliding_log::SlidingLogRateLimiter;

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
