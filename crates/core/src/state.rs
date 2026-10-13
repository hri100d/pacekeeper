use std::time::{Duration, Instant};

use crate::{Counter, Decision, WindowConfig};

pub trait State<T: Counter>: Sized {
    type Error;

    fn new(now: Instant) -> Self;

    fn try_accept(
        &mut self,
        now: Instant,
        limit: T,
        window: Duration,
    ) -> Result<Decision<T>, Self::Error>;

    fn validate_config_change(
        _current: &WindowConfig<T>,
        _next: &WindowConfig<T>,
    ) -> Result<(), Self::Error> {
        Ok(())
    }
}
