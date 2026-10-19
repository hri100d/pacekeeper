use std::{
    collections::VecDeque,
    time::{Duration, Instant},
};

use pacekeeper_core::{Counter, Decision, State};

use crate::WindowStateError;

pub(crate) struct WindowState {
    pub(crate) accepted_at: VecDeque<Instant>,
}

impl<T: Counter> State<T> for WindowState {
    type Error = WindowStateError;

    fn new(_now: Instant) -> Self {
        Self {
            accepted_at: VecDeque::new(),
        }
    }

    fn try_accept(
        &mut self,
        now: Instant,
        limit: T,
        window: Duration,
    ) -> Result<Decision<T>, Self::Error> {
        while let Some(&oldest) = self.accepted_at.front() {
            if now.duration_since(oldest) < window {
                break;
            }

            self.accepted_at.pop_front();
        }

        let accepted = T::from(self.accepted_at.len()).ok_or(WindowStateError::CounterOverflow)?;

        if accepted >= limit {
            let limit_usize = limit.to_usize().ok_or(WindowStateError::CounterOverflow)?;

            let index = self.accepted_at.len() - limit_usize;
            let timestamp = self.accepted_at[index];

            return Ok(Decision::Denied {
                retry_after: window.saturating_sub(now.duration_since(timestamp)),
            });
        }

        self.accepted_at.push_back(now);

        let oldest = *self
            .accepted_at
            .front()
            .expect("the queue contains the request just accepted");

        Ok(Decision::Allowed {
            remaining: limit - accepted - T::one(),
            reset_after: window.saturating_sub(now.duration_since(oldest)),
        })
    }
}

#[cfg(test)]
mod window_state_tests {
    use super::*;

    const WINDOW: Duration = Duration::from_secs(60);

    fn new_state(now: Instant) -> WindowState {
        <WindowState as State<u8>>::new(now)
    }

    fn acquire(state: &mut WindowState, now: Instant, limit: u8) -> Decision<u8> {
        state.try_accept(now, limit, WINDOW).unwrap()
    }

    fn allowed(remaining: u8, seconds: u64) -> Decision<u8> {
        Decision::Allowed {
            remaining,
            reset_after: Duration::from_secs(seconds),
        }
    }

    fn denied(seconds: u64) -> Decision<u8> {
        Decision::Denied {
            retry_after: Duration::from_secs(seconds),
        }
    }

    #[test]
    fn starts_empty_and_counts_the_first_request_once() {
        let now = Instant::now();
        let mut state = new_state(now);

        assert!(state.accepted_at.is_empty());
        assert_eq!(acquire(&mut state, now, 3), allowed(2, 60));
        assert_eq!(state.accepted_at, VecDeque::from([now]));
    }

    #[test]
    fn allows_the_limit_and_does_not_record_denied_requests() {
        let now = Instant::now();
        let mut state = new_state(now);

        for remaining in [2, 1, 0] {
            assert_eq!(acquire(&mut state, now, 3), allowed(remaining, 60));
        }

        let before = state.accepted_at.clone();

        for _ in 0..3 {
            assert_eq!(acquire(&mut state, now, 3), denied(60));
            assert_eq!(state.accepted_at, before);
        }
    }

    #[test]
    fn expires_at_the_exact_boundary_but_not_before() {
        let start = Instant::now();
        let mut state = new_state(start);

        acquire(&mut state, start, 1);

        assert_eq!(
            acquire(&mut state, start + WINDOW - Duration::from_nanos(1), 1,),
            Decision::Denied {
                retry_after: Duration::from_nanos(1),
            }
        );

        assert_eq!(acquire(&mut state, start + WINDOW, 1), allowed(0, 60));

        assert_eq!(state.accepted_at, VecDeque::from([start + WINDOW]));
    }

    #[test]
    fn staggered_requests_expire_individually() {
        let start = Instant::now();
        let mut state = new_state(start);

        for seconds in [0, 10, 20] {
            acquire(&mut state, start + Duration::from_secs(seconds), 3);
        }

        assert_eq!(
            acquire(&mut state, start + Duration::from_secs(59), 3),
            denied(1)
        );

        assert_eq!(
            acquire(&mut state, start + Duration::from_secs(60), 3),
            allowed(0, 10)
        );

        assert_eq!(
            acquire(&mut state, start + Duration::from_secs(61), 3),
            denied(9)
        );

        assert_eq!(
            state.accepted_at,
            VecDeque::from([
                start + Duration::from_secs(10),
                start + Duration::from_secs(20),
                start + Duration::from_secs(60),
            ])
        );
    }

    #[test]
    fn removes_multiple_expired_requests_in_one_call() {
        let start = Instant::now();
        let mut state = new_state(start);

        for seconds in [0, 10, 50] {
            acquire(&mut state, start + Duration::from_secs(seconds), 3);
        }

        assert_eq!(
            acquire(&mut state, start + Duration::from_secs(70), 3),
            allowed(1, 40)
        );

        assert_eq!(
            state.accepted_at,
            VecDeque::from([
                start + Duration::from_secs(50),
                start + Duration::from_secs(70),
            ])
        );
    }

    #[test]
    fn clears_old_history_after_a_long_idle_period() {
        let start = Instant::now();
        let mut state = new_state(start);

        for _ in 0..3 {
            acquire(&mut state, start, 3);
        }

        let later = start + Duration::from_secs(605);

        assert_eq!(acquire(&mut state, later, 3), allowed(2, 60));
        assert_eq!(state.accepted_at, VecDeque::from([later]));
    }

    #[test]
    fn increasing_limit_preserves_existing_timestamps() {
        let start = Instant::now();
        let mut state = new_state(start);

        acquire(&mut state, start, 1);

        let later = start + Duration::from_secs(20);

        assert_eq!(acquire(&mut state, later, 2), allowed(0, 40));
        assert_eq!(state.accepted_at, VecDeque::from([start, later]));
    }

    #[test]
    fn lowering_limit_waits_for_enough_requests_to_expire() {
        let start = Instant::now();
        let mut state = new_state(start);

        for seconds in [0, 10, 20, 30, 40] {
            acquire(&mut state, start + Duration::from_secs(seconds), 5);
        }

        assert_eq!(
            acquire(&mut state, start + Duration::from_secs(40), 3),
            denied(40)
        );

        assert_eq!(
            acquire(&mut state, start + Duration::from_secs(60), 3),
            denied(20)
        );

        assert_eq!(
            acquire(&mut state, start + Duration::from_secs(80), 3),
            allowed(0, 10)
        );
    }

    #[test]
    fn maximum_counter_does_not_overflow() {
        let now = Instant::now();
        let mut state = new_state(now);

        for accepted in 1..=u8::MAX {
            assert_eq!(
                acquire(&mut state, now, u8::MAX),
                allowed(u8::MAX - accepted, 60)
            );
        }

        for _ in 0..3 {
            assert_eq!(acquire(&mut state, now, u8::MAX), denied(60));
        }

        assert_eq!(state.accepted_at.len(), usize::from(u8::MAX));
    }

    #[test]
    fn count_conversion_failure_does_not_insert_a_request() {
        let now = Instant::now();
        let mut state = new_state(now);

        for _ in 0..256 {
            state.try_accept(now, 256u16, WINDOW).unwrap();
        }

        assert!(matches!(
            state.try_accept(now, u8::MAX, WINDOW),
            Err(WindowStateError::CounterOverflow)
        ));

        assert_eq!(state.accepted_at.len(), 256);
    }
}
