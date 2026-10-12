use std::time::{Duration, Instant};

use pacekeeper_core::{Counter, Decision};

use crate::WindowError;

#[derive(Debug)]
pub(crate) struct WindowStats<T: Counter = u64> {
    pub(crate) started_at: Instant,
    pub(crate) accepted: T,
}

impl<T: Counter> WindowStats<T> {
    pub fn new(now: Instant) -> Self {
        Self {
            started_at: now,
            accepted: T::zero(),
        }
    }

    pub fn try_accept(
        &mut self,
        now: Instant,
        limit: T,
        window: Duration,
    ) -> Result<Decision<T>, WindowError> {
        if now.duration_since(self.started_at) >= window {
            *self = Self::new(now);
        }

        let elapsed = now.duration_since(self.started_at);

        let reset_after = window
            .checked_sub(elapsed)
            .ok_or(WindowError::DurationOverflow)?;

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
