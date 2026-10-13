use std::time::{Duration, Instant};

use pacekeeper_core::{Counter, Decision, State};

use crate::WindowStateError;

#[derive(Debug)]
pub struct WindowState<T: Counter = u64> {
    pub(crate) started_at: Instant,
    pub(crate) accepted: T,
}

impl<T: Counter> State<T> for WindowState<T> {
    type Error = WindowStateError;

    fn new(now: Instant) -> Self {
        Self {
            started_at: now,
            accepted: T::zero(),
        }
    }

    fn try_accept(
        &mut self,
        now: Instant,
        limit: T,
        window: Duration,
    ) -> Result<Decision<T>, Self::Error> {
        if now.duration_since(self.started_at) >= window {
            *self = Self::new(now);
        }

        let elapsed = now.duration_since(self.started_at);

        let reset_after = window
            .checked_sub(elapsed)
            .ok_or(WindowStateError::DurationOverflow)?;

        if self.accepted >= limit {
            return Ok(Decision::Denied {
                retry_after: reset_after,
            });
        }

        self.accepted = self.accepted + T::one();

        Ok(Decision::Allowed {
            remaining: limit - self.accepted,
            reset_after,
        })
    }
}

#[cfg(test)]
mod window_stats_tests {
    use super::*;

    #[test]
    fn resets_at_the_exact_window_boundary() {
        let start = Instant::now();
        let window = Duration::from_secs(60);
        let mut state = WindowState::<u8>::new(start);

        state.try_accept(start, 1, window).unwrap();

        let decision = state.try_accept(start + window, 1, window).unwrap();

        assert_eq!(
            decision,
            Decision::Allowed {
                remaining: 0,
                reset_after: window,
            },
        );
    }

    #[test]
    fn allows_requests_until_limit() {
        let start = Instant::now();
        let window = Duration::from_secs(60);
        let mut state = WindowState::<u8>::new(start);
        let limit = 3_u8;

        for chance in 1..=limit {
            let response = state.try_accept(start, limit, window).unwrap();

            assert_eq!(
                response,
                Decision::Allowed {
                    remaining: limit - chance,
                    reset_after: window
                }
            );
        }
    }

    #[test]
    fn denied_requests_do_not_consume_quota() {
        let start = Instant::now();
        let window = Duration::from_secs(60);
        let mut state = WindowState::<u8>::new(start);
        let limit = 1_u8;

        assert_eq!(
            state.try_accept(start, limit, window).unwrap(),
            Decision::Allowed {
                remaining: 0,
                reset_after: window,
            }
        );

        for _ in 0..3 {
            assert_eq!(
                state.try_accept(start, limit, window).unwrap(),
                Decision::Denied {
                    retry_after: window,
                }
            );
            assert_eq!(state.accepted, limit);
            assert_eq!(state.started_at, start);
        }
    }

    #[test]
    fn denies_just_before_window_boundary() {
        let start = Instant::now();
        let window = Duration::from_secs(60);
        let mut state = WindowState::<u8>::new(start);
        let limit = 1_u8;

        for chance in 1..=limit {
            let response = state.try_accept(start, limit, window).unwrap();

            assert_eq!(
                response,
                Decision::Allowed {
                    remaining: limit - chance,
                    reset_after: window
                }
            );
        }

        let one_nanosec = Duration::from_nanos(1);

        let response = state
            .try_accept(start + window - one_nanosec, limit, window)
            .unwrap();

        assert_eq!(
            response,
            Decision::Denied {
                retry_after: one_nanosec
            }
        );
    }

    #[test]
    fn resets_after_long_idle_period() {
        let start = Instant::now();
        let window = Duration::from_secs(60);
        let mut state = WindowState::<u8>::new(start);

        assert!(matches!(
            state.try_accept(start, 1, window).unwrap(),
            Decision::Allowed { .. }
        ));

        assert!(matches!(
            state.try_accept(start, 1, window).unwrap(),
            Decision::Denied { .. }
        ));

        let later = start + Duration::from_secs(605);

        assert_eq!(
            state.try_accept(later, 1, window).unwrap(),
            Decision::Allowed {
                remaining: 0,
                reset_after: window,
            }
        );
        assert_eq!(state.started_at, later);
        assert_eq!(state.accepted, 1);
    }

    #[test]
    fn calculates_remaining_time_from_supplied_timestamp() {
        let start = Instant::now();
        let window = Duration::from_secs(60);
        let mut state = WindowState::<u8>::new(start);

        state.try_accept(start, 2, window).unwrap();

        let now = start + Duration::from_secs(20);

        assert_eq!(
            state.try_accept(now, 2, window).unwrap(),
            Decision::Allowed {
                remaining: 0,
                reset_after: Duration::from_secs(40),
            }
        );
        assert_eq!(
            state.try_accept(now, 2, window).unwrap(),
            Decision::Denied {
                retry_after: Duration::from_secs(40),
            }
        );
    }

    #[test]
    fn handles_maximum_counter_without_overflow() {
        let start = Instant::now();
        let window = Duration::from_secs(60);
        let mut state = WindowState::<u8>::new(start);
        let limit = u8::MAX;

        for accepted in 1..=limit {
            assert_eq!(
                state.try_accept(start, limit, window).unwrap(),
                Decision::Allowed {
                    remaining: limit - accepted,
                    reset_after: window,
                }
            );
        }

        for _ in 0..3 {
            assert_eq!(
                state.try_accept(start, limit, window).unwrap(),
                Decision::Denied {
                    retry_after: window,
                }
            );
            assert_eq!(state.accepted, u8::MAX);
        }
    }
}
