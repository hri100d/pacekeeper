use std::time::{Duration, Instant};

use pacekeeper_core::{Counter, Decision, State, WindowConfig};

use crate::WindowStateError;

#[derive(Debug)]
pub struct WindowState<T: Counter = u64> {
    current_started_at: Instant,
    current_accepted: T,
    previous_accepted: T,
}

impl<T: Counter> State<T> for WindowState<T> {
    type Error = WindowStateError;

    fn new(now: Instant) -> Self {
        Self {
            current_started_at: now,
            current_accepted: T::zero(),
            previous_accepted: T::zero(),
        }
    }

    fn validate_config_change(
        current: &WindowConfig<T>,
        next: &WindowConfig<T>,
    ) -> Result<(), Self::Error> {
        if current.window() != next.window() {
            return Err(WindowStateError::WindowChangeNotAllowed);
        }

        Ok(())
    }

    fn try_accept(
        &mut self,
        now: Instant,
        limit: T,
        window: std::time::Duration,
    ) -> Result<Decision<T>, Self::Error> {
        let window_ns = window.as_nanos();
        let elapsed_ns = now.duration_since(self.current_started_at).as_nanos();

        let passed_windows = elapsed_ns / window_ns;
        let within_window_ns = elapsed_ns % window_ns;
        if passed_windows > 0 {
            let new_start = now
                .checked_sub(duration_from_nanos(within_window_ns)?)
                .ok_or(WindowStateError::DurationOverflow)?;

            self.previous_accepted = if passed_windows == 1 {
                self.current_accepted
            } else {
                T::zero()
            };

            self.current_accepted = T::zero();
            self.current_started_at = new_start;
        }

        let remaining_ns = window_ns - within_window_ns;

        let limit = limit.to_u128().ok_or(WindowStateError::CounterOverflow)?;

        let current = self
            .current_accepted
            .to_u128()
            .ok_or(WindowStateError::CounterOverflow)?;

        let previous = self
            .previous_accepted
            .to_u128()
            .ok_or(WindowStateError::CounterOverflow)?;

        let weighted_previous = checked_product(previous, remaining_ns)?.div_ceil(window_ns);

        let available = limit
            .saturating_sub(current)
            .saturating_sub(weighted_previous);

        if available == 0 {
            let retry_ns = if current < limit {
                let budget = limit - current - 1;

                let permitted_remaining = checked_product(budget, window_ns)? / previous;

                remaining_ns - permitted_remaining
            } else {
                let budget = limit - 1;

                let permitted_remaining = checked_product(budget, window_ns)? / current;

                remaining_ns + window_ns - permitted_remaining
            };

            return Ok(Decision::Denied {
                retry_after: duration_from_nanos(retry_ns)?,
            });
        }

        let remaining = T::from(available - 1).ok_or(WindowStateError::CounterOverflow)?;

        let reset_after = duration_from_nanos(remaining_ns)?;

        self.current_accepted = self.current_accepted + T::one();

        Ok(Decision::Allowed {
            remaining,
            reset_after,
        })
    }
}

fn duration_from_nanos(nanos: u128) -> Result<Duration, WindowStateError> {
    let seconds =
        u64::try_from(nanos / 1_000_000_000).map_err(|_| WindowStateError::DurationOverflow)?;

    let subsecond = (nanos % 1_000_000_000) as u32;

    Ok(Duration::new(seconds, subsecond))
}

fn checked_product(a: u128, b: u128) -> Result<u128, WindowStateError> {
    a.checked_mul(b).ok_or(WindowStateError::ArithmeticOverflow)
}

#[cfg(test)]
mod window_stats_tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(60);

    fn full_state(start: Instant, limit: u8) -> WindowState<u8> {
        let mut state = WindowState::<u8>::new(start);

        for _ in 0..limit {
            state.try_accept(start, limit, WINDOW).unwrap();
        }

        state
    }

    #[test]
    fn starts_empty_and_counts_only_allowed_requests() {
        let start = Instant::now();
        let mut state = WindowState::<u8>::new(start);

        assert_eq!(state.current_accepted, 0);
        assert_eq!(state.previous_accepted, 0);

        for remaining in [2, 1, 0] {
            assert_eq!(
                state.try_accept(start, 3, WINDOW).unwrap(),
                Decision::Allowed {
                    remaining,
                    reset_after: WINDOW,
                }
            );
        }

        for _ in 0..3 {
            assert!(matches!(
                state.try_accept(start, 3, WINDOW).unwrap(),
                Decision::Denied { .. }
            ));
        }

        assert_eq!(state.current_accepted, 3);
        assert_eq!(state.previous_accepted, 0);
        assert_eq!(state.current_started_at, start);
    }

    #[test]
    fn boundary_moves_current_usage_into_previous_bucket() {
        let start = Instant::now();
        let mut state = full_state(start, 3);

        assert_eq!(
            state.try_accept(start + WINDOW, 3, WINDOW).unwrap(),
            Decision::Denied {
                retry_after: Duration::from_secs(20),
            }
        );

        assert_eq!(state.current_started_at, start + WINDOW);
        assert_eq!(state.previous_accepted, 3);
        assert_eq!(state.current_accepted, 0);
    }

    #[test]
    fn fractional_usage_is_denied_until_the_exact_retry_time() {
        let start = Instant::now();
        let mut state = full_state(start, 3);

        assert_eq!(
            state
                .try_accept(start + Duration::from_secs(70), 3, WINDOW)
                .unwrap(),
            Decision::Denied {
                retry_after: Duration::from_secs(10),
            }
        );

        let retry_at = start + Duration::from_secs(80);

        assert_eq!(
            state
                .try_accept(retry_at - Duration::from_nanos(1), 3, WINDOW)
                .unwrap(),
            Decision::Denied {
                retry_after: Duration::from_nanos(1),
            }
        );

        assert_eq!(
            state.try_accept(retry_at, 3, WINDOW).unwrap(),
            Decision::Allowed {
                remaining: 0,
                reset_after: Duration::from_secs(40),
            }
        );

        assert_eq!(state.previous_accepted, 3);
        assert_eq!(state.current_accepted, 1);
    }

    #[test]
    fn two_or_more_elapsed_windows_clear_history_and_preserve_alignment() {
        for (seconds, bucket_start, reset_seconds) in [(120, 120, 60), (605, 600, 55)] {
            let start = Instant::now();
            let mut state = full_state(start, 3);

            assert_eq!(
                state
                    .try_accept(start + Duration::from_secs(seconds), 3, WINDOW,)
                    .unwrap(),
                Decision::Allowed {
                    remaining: 2,
                    reset_after: Duration::from_secs(reset_seconds),
                }
            );

            assert_eq!(
                state.current_started_at,
                start + Duration::from_secs(bucket_start)
            );
            assert_eq!(state.previous_accepted, 0);
            assert_eq!(state.current_accepted, 1);
        }
    }

    #[test]
    fn limit_one_requires_two_windows_for_a_request_at_bucket_start() {
        let start = Instant::now();
        let mut state = full_state(start, 1);

        for seconds in [0, 60, 119] {
            assert_eq!(
                state
                    .try_accept(start + Duration::from_secs(seconds), 1, WINDOW,)
                    .unwrap(),
                Decision::Denied {
                    retry_after: Duration::from_secs(120 - seconds),
                }
            );
        }

        assert_eq!(
            state
                .try_accept(start + Duration::from_secs(120), 1, WINDOW)
                .unwrap(),
            Decision::Allowed {
                remaining: 0,
                reset_after: WINDOW,
            }
        );
    }

    #[test]
    fn lowering_limit_waits_for_current_usage_to_decay_in_next_bucket() {
        let start = Instant::now();
        let mut state = full_state(start, 5);

        assert_eq!(
            state.try_accept(start, 2, WINDOW).unwrap(),
            Decision::Denied {
                retry_after: Duration::from_secs(108),
            }
        );

        let retry_at = start + Duration::from_secs(108);

        assert_eq!(
            state
                .try_accept(retry_at - Duration::from_nanos(1), 2, WINDOW)
                .unwrap(),
            Decision::Denied {
                retry_after: Duration::from_nanos(1),
            }
        );

        assert_eq!(
            state.try_accept(retry_at, 2, WINDOW).unwrap(),
            Decision::Allowed {
                remaining: 0,
                reset_after: Duration::from_secs(12),
            }
        );
    }

    #[test]
    fn maximum_u8_count_and_subsecond_windows_work() {
        let start = Instant::now();
        let mut state = WindowState::<u8>::new(start);
        let window = Duration::from_nanos(255);

        for accepted in 1..=u8::MAX {
            assert_eq!(
                state.try_accept(start, u8::MAX, window).unwrap(),
                Decision::Allowed {
                    remaining: u8::MAX - accepted,
                    reset_after: window,
                }
            );
        }

        assert_eq!(
            state.try_accept(start, u8::MAX, window).unwrap(),
            Decision::Denied {
                retry_after: Duration::from_nanos(256),
            }
        );

        assert_eq!(state.current_accepted, u8::MAX);

        assert_eq!(
            state
                .try_accept(start + Duration::from_nanos(256), u8::MAX, window,)
                .unwrap(),
            Decision::Allowed {
                remaining: 0,
                reset_after: Duration::from_nanos(254),
            }
        );
    }

    #[test]
    fn large_u128_limits_do_not_lose_integer_precision() {
        let start = Instant::now();
        let mut state = WindowState::<u128>::new(start);

        assert_eq!(
            state.try_accept(start, u128::MAX, WINDOW).unwrap(),
            Decision::Allowed {
                remaining: u128::MAX - 1,
                reset_after: WINDOW,
            }
        );
    }

    #[test]
    fn overflowing_weight_or_retry_math_returns_an_error_without_accepting() {
        let now = Instant::now();

        for (current, previous) in [(0, u128::MAX), (u128::MAX, 0)] {
            let mut state = WindowState::<u128> {
                current_started_at: now,
                current_accepted: current,
                previous_accepted: previous,
            };

            assert!(matches!(
                state.try_accept(now, u128::MAX, Duration::from_nanos(2)),
                Err(WindowStateError::ArithmeticOverflow)
            ));

            assert_eq!(state.current_accepted, current);
            assert_eq!(state.previous_accepted, previous);
        }
    }

    #[test]
    fn unrepresentable_retry_duration_returns_an_error() {
        let now = Instant::now();
        let mut state = WindowState::<u8>::new(now);

        state.try_accept(now, 1, Duration::MAX).unwrap();

        assert!(matches!(
            state.try_accept(now, 1, Duration::MAX),
            Err(WindowStateError::DurationOverflow)
        ));

        assert_eq!(state.current_accepted, 1);
    }

    #[test]
    fn decisions_and_retry_times_match_a_small_reference_model() {
        fn mass(history: &[u64], now: u64, window: u64) -> u64 {
            let bucket = now / window;

            let current = history.iter().filter(|&&t| t / window == bucket).count() as u64;

            let previous = history
                .iter()
                .filter(|&&t| bucket > 0 && t / window == bucket - 1)
                .count() as u64;

            current * window + previous * (window - now % window)
        }

        for window in 1u64..=8 {
            for initial_limit in 1u8..=5 {
                let start = Instant::now();
                let mut state = WindowState::<u8>::new(start);
                let mut history = Vec::new();

                for tick in (0u64..=40).chain([75, 76, 100, 101]) {
                    let limit = 1 + (initial_limit - 1 + (tick / 7) as u8) % 5;

                    let budget = u64::from(limit) * window;
                    let fits = mass(&history, tick, window) + window <= budget;

                    let actual = state
                        .try_accept(
                            start + Duration::from_nanos(tick),
                            limit,
                            Duration::from_nanos(window),
                        )
                        .unwrap();

                    let expected = if fits {
                        history.push(tick);

                        Decision::Allowed {
                            remaining: ((budget - mass(&history, tick, window)) / window) as u8,
                            reset_after: Duration::from_nanos(window - tick % window),
                        }
                    } else {
                        let wait = (1..=2 * window)
                            .find(|&wait| mass(&history, tick + wait, window) + window <= budget)
                            .unwrap();

                        Decision::Denied {
                            retry_after: Duration::from_nanos(wait),
                        }
                    };

                    assert_eq!(
                        actual, expected,
                        "window={window}, tick={tick}, limit={limit}"
                    );
                }
            }
        }
    }
}
