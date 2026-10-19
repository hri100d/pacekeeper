use std::time::{Duration, Instant};

use pacekeeper_core::Decision;

use crate::{BucketConfig, BucketCounter, BucketError};

pub(crate) struct BucketState {
    token_units: u64,
    accounted_at: Instant,
    last_seen: Instant,
}

impl BucketState {
    pub(crate) fn new<C: BucketCounter, R: BucketCounter>(
        config: &BucketConfig<C, R>,
        now: Instant,
    ) -> Self {
        Self {
            token_units: config.max_credit(),
            accounted_at: now,
            last_seen: now,
        }
    }

    pub(crate) fn try_accept<C: BucketCounter, R: BucketCounter>(
        &mut self,
        now: Instant,
        config: &BucketConfig<C, R>,
    ) -> Result<Decision<C>, BucketError> {
        if now < self.last_seen {
            return Err(BucketError::ClockWentBackwards);
        }

        let elapsed_us = now.duration_since(self.accounted_at).as_micros();

        let capacity_units = config.max_credit();
        let units_per_token = config.period_us();
        let refill_per_us = config.refill_tokens_u64();

        let missing_units = capacity_units - self.token_units;
        let time_until_full_us = missing_units.div_ceil(refill_per_us);

        if elapsed_us >= u128::from(time_until_full_us) {
            self.token_units = capacity_units;

            self.accounted_at = now;
        } else {
            let elapsed_us = elapsed_us as u64;

            self.token_units += elapsed_us * refill_per_us;

            self.accounted_at += Duration::from_micros(elapsed_us);
        }

        self.last_seen = now;

        if self.token_units < units_per_token {
            return Ok(Decision::Denied {
                retry_after: Duration::from_micros(
                    (units_per_token - self.token_units).div_ceil(refill_per_us),
                ),
            });
        }

        self.token_units -= units_per_token;

        Ok(Decision::Allowed {
            remaining: C::from(self.token_units / units_per_token)
                .expect("remaining tokens cannot exceed the validated capacity"),
            reset_after: Duration::from_micros(
                (capacity_units - self.token_units).div_ceil(refill_per_us),
            ),
        })
    }
}

impl<C: BucketCounter, R: BucketCounter> pacekeeper_core::InMemoryState<BucketConfig<C, R>>
    for BucketState
{
    type Counter = C;
    type Error = BucketError;

    fn new(config: &BucketConfig<C, R>, now: Instant) -> Self {
        BucketState::new(config, now)
    }

    fn try_accept(
        &mut self,
        now: Instant,
        config: &BucketConfig<C, R>,
    ) -> Result<Decision<C>, Self::Error> {
        BucketState::try_accept(self, now, config)
    }
}
