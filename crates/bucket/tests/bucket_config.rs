use std::time::Duration;

use pacekeeper_bucket::{BucketConfig, ConfigError};

#[test]
fn rejects_zero_values_and_submicrosecond_periods() {
    let cases = [
        (
            0u64,
            1u64,
            Duration::from_secs(1),
            ConfigError::ZeroCapacity,
        ),
        (1, 0, Duration::from_secs(1), ConfigError::ZeroRefillTokens),
        (1, 1, Duration::ZERO, ConfigError::ZeroRefillPeriod),
        (
            1,
            1,
            Duration::from_nanos(1_500),
            ConfigError::SubmicrosecondPrecision,
        ),
    ];

    for (capacity, refill, period, expected) in cases {
        assert_eq!(
            BucketConfig::new(capacity, refill, period).unwrap_err(),
            expected,
        );
    }
}

#[test]
fn enforces_the_exact_arithmetic_boundary() {
    let max_exact = (1u64 << 52) - 1;
    let period = Duration::from_micros(1);

    assert!(BucketConfig::new(max_exact, max_exact, period).is_ok());

    assert_eq!(
        BucketConfig::new(max_exact + 1, 1u64, period).unwrap_err(),
        ConfigError::CreditOutOfRange,
    );

    assert_eq!(
        BucketConfig::new(1u64, max_exact + 1, period).unwrap_err(),
        ConfigError::RefillOutOfRange,
    );
}

#[test]
fn rejects_period_and_multiplication_overflow() {
    assert_eq!(
        BucketConfig::new(1u64, 1u64, Duration::from_secs(u64::MAX),).unwrap_err(),
        ConfigError::PeriodTooLarge,
    );

    assert_eq!(
        BucketConfig::new(u64::MAX, 1u64, Duration::from_micros(2),).unwrap_err(),
        ConfigError::CreditOverflow,
    );
}
