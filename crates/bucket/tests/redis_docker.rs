#![cfg(any(feature = "redis", feature = "redis-cluster"))]

mod support;

use std::{sync::Arc, time::Duration};

use pacekeeper_bucket::{BucketConfig, BucketError, Decision, RedisTokenBucket};
use tokio::{
    sync::Barrier,
    task::JoinSet,
    time::{sleep, timeout},
};

use support::{RedisFixture, TestResult};

#[cfg(feature = "redis")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker"]
async fn standalone_redis_contract() -> TestResult {
    let fixture = RedisFixture::standalone().await?;

    timeout(Duration::from_secs(45), verify_backend(&fixture)).await??;

    fixture.container.rm().await?;
    Ok(())
}

#[cfg(feature = "redis-cluster")]
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
#[ignore = "requires Docker"]
async fn redis_cluster_contract() -> TestResult {
    let fixture = RedisFixture::cluster().await?;

    timeout(Duration::from_secs(45), verify_backend(&fixture)).await??;

    fixture.container.rm().await?;
    Ok(())
}

fn assert_allowed(decision: Decision<u64>, expected_remaining: u64) {
    match decision {
        Decision::Allowed {
            remaining,
            reset_after,
        } => {
            assert_eq!(remaining, expected_remaining);
            assert!(reset_after > Duration::ZERO);
        }

        other => panic!("Expected Allowed, got {other:?}"),
    }
}

fn stored_key(namespace: &str, key: &str) -> String {
    format!("pacekeeper:bucket:v1:{}:{namespace}:{key}", namespace.len(),)
}

async fn verify_backend(fixture: &RedisFixture) -> TestResult {
    let period = Duration::from_secs(3_600);
    let config = BucketConfig::new(5u64, 1u64, period)?;

    let first = Arc::new(fixture.connect("shared", config).await?);
    let second = Arc::new(fixture.connect("shared", config).await?);

    assert_allowed(first.try_acquire("user").await?, 4);
    assert_allowed(second.try_acquire("user").await?, 3);

    let different_config = BucketConfig::new(6u64, 1u64, period)?;
    let incompatible = fixture.connect("shared", different_config).await?;

    assert!(matches!(
        incompatible.try_acquire("user").await,
        Err(BucketError::ConfigMismatch),
    ));

    assert_allowed(first.try_acquire("user").await?, 2);

    let separate = fixture.connect("separate", config).await?;
    assert_allowed(separate.try_acquire("user").await?, 4);

    fixture.flush_scripts().await?;
    assert_allowed(second.try_acquire("user").await?, 1);

    assert!(first.remove_key("user").await?);
    assert!(!first.remove_key("user").await?);
    assert_allowed(second.try_acquire("user").await?, 4);

    assert_allowed(separate.try_acquire("user").await?, 3);

    assert_allowed(first.try_acquire("corrupt").await?, 4);

    let mut corrupt = redis::cmd("HSET");
    corrupt
        .arg(stored_key("shared", "corrupt"))
        .arg("token_units")
        .arg("not-a-number");

    let _: u64 = fixture.query(&corrupt).await?;

    assert!(matches!(
        first.try_acquire("corrupt").await,
        Err(BucketError::InvalidRedisState),
    ));

    let worker_count = 32;
    let barrier = Arc::new(Barrier::new(worker_count + 1));
    let mut workers = JoinSet::new();

    for index in 0..worker_count {
        let limiter = Arc::clone(if index % 2 == 0 { &first } else { &second });

        let barrier = Arc::clone(&barrier);

        workers.spawn(async move {
            barrier.wait().await;
            limiter.try_acquire("concurrent").await
        });
    }

    barrier.wait().await;

    let mut allowed = 0;
    let mut denied = 0;
    let mut remaining_values = Vec::new();

    while let Some(result) = workers.join_next().await {
        match result?? {
            Decision::Allowed {
                remaining,
                reset_after,
            } => {
                allowed += 1;
                remaining_values.push(remaining);
                assert!(reset_after > Duration::ZERO);
            }

            Decision::Denied { retry_after } => {
                denied += 1;
                assert!(retry_after > Duration::ZERO);
                assert!(retry_after <= period);
            }
        }
    }

    remaining_values.sort_unstable();

    assert_eq!(allowed, 5);
    assert_eq!(denied, 27);
    assert_eq!(remaining_values, vec![0, 1, 2, 3, 4]);

    for index in 0..128 {
        let key = format!("routing-{index}");

        assert_allowed(first.try_acquire(&key).await?, 4);
        assert_allowed(second.try_acquire(&key).await?, 3);
    }

    fixture.assert_every_node_has_keys().await?;

    expires_and_recreates_full_bucket(fixture).await?;
    refills_existing_bucket_before_expiry(fixture).await?;
    denied_requests_preserve_fractional_refill(fixture).await?;
    refill_never_exceeds_capacity(fixture).await?;
    ttl_does_not_expire_before_bucket_is_full(fixture).await?;
    future_timestamp_returns_error_without_mutation(fixture).await?;
    lua_handles_maximum_supported_values(fixture).await?;

    Ok(())
}

async fn expires_and_recreates_full_bucket(fixture: &RedisFixture) -> TestResult {
    let config = BucketConfig::new(1u64, 1u64, Duration::from_secs(1))?;
    let limiter = fixture.connect("expiry", config).await?;

    assert_allowed(limiter.try_acquire("user").await?, 0);

    let mut exists = redis::cmd("EXISTS");
    exists.arg(stored_key("expiry", "user"));

    timeout(Duration::from_secs(10), async {
        loop {
            let count: u64 = fixture.query(&exists).await?;

            if count == 0 {
                return Ok::<(), Box<dyn std::error::Error + Send + Sync>>(());
            }

            sleep(Duration::from_millis(25)).await;
        }
    })
    .await??;

    assert_allowed(limiter.try_acquire("user").await?, 0);

    Ok(())
}

#[derive(Debug)]
struct StoredBucket {
    units: u64,
    updated_at_us: u64,
}

async fn read_stored_bucket(fixture: &RedisFixture, namespace: &str) -> TestResult<StoredBucket> {
    let mut command = redis::cmd("HMGET");
    command
        .arg(stored_key(namespace, "user"))
        .arg("token_units")
        .arg("updated_at_us")
        .arg("test_marker");

    let (units, updated_at_us, marker): (u64, u64, String) = fixture.query(&command).await?;

    assert_eq!(marker, "original", "The original bucket must still exist",);

    Ok(StoredBucket {
        units,
        updated_at_us,
    })
}

async fn seed_existing_bucket(
    fixture: &RedisFixture,
    namespace: &str,
    units: u64,
    age_us: i64,
) -> TestResult {
    const SCRIPT: &str = r#"
        if redis.call("EXISTS", KEYS[1]) ~= 1 then
            return redis.error_reply("ERR test bucket does not exist")
        end

        local time = redis.call("TIME")
        local now_us = tonumber(time[1]) * 1000000 + tonumber(time[2])
        local updated_at_us = now_us - tonumber(ARGV[2])

        redis.call(
            "HSET", KEYS[1],
            "token_units", ARGV[1],
            "updated_at_us", string.format("%.0f", updated_at_us),
            "test_marker", "original"
        )

        -- Keep the prepared state alive until the production script runs.
        -- The production script must assign its own expiration again.
        redis.call("PERSIST", KEYS[1])

        return 1
    "#;

    let mut command = redis::cmd("EVAL");
    command
        .arg(SCRIPT)
        .arg(1)
        .arg(stored_key(namespace, "user"))
        .arg(units)
        .arg(age_us);

    let result: u8 = fixture.query(&command).await?;
    assert_eq!(result, 1);

    Ok(())
}

async fn age_existing_bucket(fixture: &RedisFixture, namespace: &str, age_us: i64) -> TestResult {
    assert!(age_us > 0);

    let mut command = redis::cmd("HINCRBY");
    command
        .arg(stored_key(namespace, "user"))
        .arg("updated_at_us")
        .arg(-age_us);

    let _: i64 = fixture.query(&command).await?;

    Ok(())
}

async fn acquire_and_check_arithmetic(
    fixture: &RedisFixture,
    limiter: &RedisTokenBucket,
    namespace: &str,
) -> TestResult<Decision<u64>> {
    let before = read_stored_bucket(fixture, namespace).await?;
    let decision = limiter.try_acquire("user").await?;
    let after = read_stored_bucket(fixture, namespace).await?;

    let elapsed_us = after
        .updated_at_us
        .checked_sub(before.updated_at_us)
        .expect("A successful acquisition must not move time backwards");

    let config = limiter.config();
    let period_us = config.refill_period().as_micros();
    let refill = u128::from(config.refill_tokens());
    let maximum = u128::from(config.capacity()) * period_us;

    let available = (u128::from(before.units) + u128::from(elapsed_us) * refill).min(maximum);

    let (expected_units, expected_decision) = if available >= period_us {
        let units = available - period_us;
        let full_after_us = (maximum - units).div_ceil(refill);

        (
            units,
            Decision::Allowed {
                remaining: u64::try_from(units / period_us)?,
                reset_after: Duration::from_micros(u64::try_from(full_after_us)?),
            },
        )
    } else {
        let retry_after_us = (period_us - available).div_ceil(refill);

        (
            available,
            Decision::Denied {
                retry_after: Duration::from_micros(u64::try_from(retry_after_us)?),
            },
        )
    };

    assert_eq!(
        u128::from(after.units),
        expected_units,
        "Incorrect stored token balance for {namespace}",
    );

    assert_eq!(
        decision, expected_decision,
        "Incorrect decision or wait duration for {namespace}",
    );

    Ok(decision)
}

async fn refills_existing_bucket_before_expiry(fixture: &RedisFixture) -> TestResult {
    let namespace = "refill-existing";
    let period = Duration::from_secs(3_600);
    let config = BucketConfig::new(4u64, 1u64, period)?;
    let limiter = fixture.connect(namespace, config).await?;

    assert_allowed(limiter.try_acquire("user").await?, 3);

    seed_existing_bucket(fixture, namespace, 0, 5_400_000_000).await?;

    let decision = acquire_and_check_arithmetic(fixture, &limiter, namespace).await?;

    assert_allowed(decision, 0);

    Ok(())
}

async fn denied_requests_preserve_fractional_refill(fixture: &RedisFixture) -> TestResult {
    let namespace = "fractional-refill";
    let period = Duration::from_secs(3_600);
    let config = BucketConfig::new(1u64, 1u64, period)?;
    let limiter = fixture.connect(namespace, config).await?;

    assert_allowed(limiter.try_acquire("user").await?, 0);
    seed_existing_bucket(fixture, namespace, 0, 0).await?;

    let mut previous_retry = period;

    for _ in 0..3 {
        age_existing_bucket(fixture, namespace, 900_000_000).await?;

        let decision = acquire_and_check_arithmetic(fixture, &limiter, namespace).await?;

        match decision {
            Decision::Denied { retry_after } => {
                assert!(retry_after > Duration::ZERO);
                assert!(retry_after < previous_retry);
                previous_retry = retry_after;
            }

            other => panic!("A partial token must not allow a request: {other:?}",),
        }
    }

    age_existing_bucket(fixture, namespace, 900_000_000).await?;

    let decision = acquire_and_check_arithmetic(fixture, &limiter, namespace).await?;

    assert_allowed(decision, 0);

    Ok(())
}

async fn refill_never_exceeds_capacity(fixture: &RedisFixture) -> TestResult {
    let namespace = "refill-capacity";
    let period = Duration::from_secs(3_600);
    let config = BucketConfig::new(3u64, 1u64, period)?;
    let limiter = fixture.connect(namespace, config).await?;

    assert_allowed(limiter.try_acquire("user").await?, 2);

    seed_existing_bucket(fixture, namespace, 0, 36_000_000_000).await?;

    for remaining in (0u64..3).rev() {
        let decision = acquire_and_check_arithmetic(fixture, &limiter, namespace).await?;

        assert_allowed(decision, remaining);
    }

    let decision = acquire_and_check_arithmetic(fixture, &limiter, namespace).await?;

    assert!(matches!(decision, Decision::Denied { .. }));

    Ok(())
}

async fn ttl_does_not_expire_before_bucket_is_full(fixture: &RedisFixture) -> TestResult {
    let namespace = "ttl-full-bucket";
    let period = Duration::from_secs(3_600);
    let config = BucketConfig::new(4u64, 1u64, period)?;
    let limiter = fixture.connect(namespace, config).await?;

    assert_allowed(limiter.try_acquire("user").await?, 3);
    seed_existing_bucket(fixture, namespace, 0, 0).await?;

    let decision = acquire_and_check_arithmetic(fixture, &limiter, namespace).await?;

    assert!(matches!(decision, Decision::Denied { .. }));

    const SCRIPT: &str = r#"
        local time = redis.call("TIME")
        local now_us = tonumber(time[1]) * 1000000 + tonumber(time[2])

        local state = redis.call(
            "HMGET", KEYS[1],
            "token_units",
            "updated_at_us"
        )

        return {
            redis.call("PTTL", KEYS[1]),
            now_us,
            state[1],
            state[2]
        }
    "#;

    let mut command = redis::cmd("EVAL");
    command
        .arg(SCRIPT)
        .arg(1)
        .arg(stored_key(namespace, "user"));

    let (ttl_ms, now_us, units, updated_at_us): (i64, u64, u64, u64) =
        fixture.query(&command).await?;

    assert!(ttl_ms > 0, "The script must assign an expiration");

    let elapsed_us = now_us
        .checked_sub(updated_at_us)
        .expect("Redis time must not move backwards during this test");

    let maximum = u128::from(config.capacity()) * period.as_micros();
    let refill = u128::from(config.refill_tokens());

    let available = (u128::from(units) + u128::from(elapsed_us) * refill).min(maximum);

    let full_after_us = (maximum - available).div_ceil(refill);
    let expected_ms = i64::try_from(full_after_us.div_ceil(1_000))?;

    assert!(
        ttl_ms.abs_diff(expected_ms) <= 2,
        "TTL {ttl_ms} ms does not match full-refill time \
         {expected_ms} ms",
    );

    assert!(
        ttl_ms > 3 * 3_600_000,
        "An empty four-token bucket must retain state for nearly four hours",
    );

    Ok(())
}

async fn future_timestamp_returns_error_without_mutation(fixture: &RedisFixture) -> TestResult {
    use std::collections::BTreeMap;

    let namespace = "future-timestamp";
    let config = BucketConfig::new(2u64, 1u64, Duration::from_secs(3_600))?;
    let limiter = fixture.connect(namespace, config).await?;

    assert_allowed(limiter.try_acquire("user").await?, 1);

    seed_existing_bucket(fixture, namespace, 0, -3_600_000_000).await?;

    let key = stored_key(namespace, "user");

    let mut read_hash = redis::cmd("HGETALL");
    read_hash.arg(&key);

    let before: BTreeMap<String, String> = fixture.query(&read_hash).await?;

    assert!(matches!(
        limiter.try_acquire("user").await,
        Err(BucketError::ClockWentBackwards),
    ));

    let after: BTreeMap<String, String> = fixture.query(&read_hash).await?;

    assert_eq!(
        after, before,
        "An error must not change any stored bucket fields",
    );

    let mut read_ttl = redis::cmd("PTTL");
    read_ttl.arg(&key);

    let ttl: i64 = fixture.query(&read_ttl).await?;
    assert_eq!(ttl, -1);

    Ok(())
}

async fn lua_handles_maximum_supported_values(fixture: &RedisFixture) -> TestResult {
    const MAX_EXACT: u64 = (1u64 << 52) - 1;

    for refill in [1u64, 1_000_000] {
        let namespace = format!("maximum-arithmetic-{refill}");
        let config = BucketConfig::new(1u64, refill, Duration::from_micros(MAX_EXACT))?;

        let limiter = fixture.connect(&namespace, config).await?;

        assert_allowed(limiter.try_acquire("user").await?, 0);

        seed_existing_bucket(fixture, &namespace, 0, 3_600_000_000).await?;

        let decision = acquire_and_check_arithmetic(fixture, &limiter, &namespace).await?;

        assert!(matches!(decision, Decision::Denied { .. }));
    }

    let config = BucketConfig::new(MAX_EXACT, 1u64, Duration::from_micros(1))?;

    let limiter = fixture.connect("maximum-capacity", config).await?;

    assert_eq!(
        limiter.try_acquire("user").await?,
        Decision::Allowed {
            remaining: MAX_EXACT - 1,
            reset_after: Duration::from_micros(1),
        },
    );

    let config = BucketConfig::new(1u64, MAX_EXACT, Duration::from_micros(MAX_EXACT))?;

    let limiter = fixture.connect("maximum-refill", config).await?;

    assert_eq!(
        limiter.try_acquire("user").await?,
        Decision::Allowed {
            remaining: 0,
            reset_after: Duration::from_micros(1),
        },
    );

    Ok(())
}
