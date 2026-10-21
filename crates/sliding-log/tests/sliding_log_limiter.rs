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

#[test]
fn explicit_shards_validate_counts_and_preserve_key_usage() {
    let window = Duration::from_secs(60);
    for count in [0, 3, 6] {
        let config = pacekeeper_core::WindowConfig::new(1u64, window).unwrap();
        assert!(matches!(
            SlidingLogRateLimiter::<String>::new_with_config_and_shards(config, count),
            Err(pacekeeper_core::InvalidShardCount)
        ));
    }
    for count in [1, 2, 8, 64] {
        let config = pacekeeper_core::WindowConfig::new(1u64, window).unwrap();
        let limiter =
            SlidingLogRateLimiter::<String>::new_with_config_and_shards(config, count).unwrap();
        let now = Instant::now();
        for key in ["a", "b"] {
            assert!(matches!(
                limiter.try_acquire_at(key, now).unwrap(),
                Decision::Allowed { remaining: 0, .. }
            ));
            assert!(matches!(
                limiter.try_acquire_at(key, now).unwrap(),
                Decision::Denied { .. }
            ));
        }
        assert!(limiter.remove_key("a").unwrap());
        assert!(matches!(
            limiter.try_acquire_at("a", now).unwrap(),
            Decision::Allowed { .. }
        ));
        assert!(matches!(
            limiter.try_acquire_at("b", now).unwrap(),
            Decision::Denied { .. }
        ));
    }
}
