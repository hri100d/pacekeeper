use std::time::{Duration, Instant};

use crate::{Counter, Decision, WindowConfig};

/// Per-key state for a window-based rate-limiting algorithm.
///
/// `new` initializes state without consuming a request.
/// The engine subsequently calls `try_accept` for the incoming request.
///
/// Configuration updates preserve states. Override
/// `validate_config_change` to reject incompatible changes.
///
/// Implementations automatically receive an
/// `InMemoryState<WindowConfig<T>>` implementation.
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

/// Per-key state used by the generic in-memory engine.
///
/// `new` initializes state without consuming a request.
/// `try_accept` evaluates and records one acquisition.
///
/// Methods run while the engine's mutex is held. Avoid blocking I/O or
/// re-entering the same limiter from these methods.
///
/// Override `validate_config_change` when existing state cannot safely
/// interpret a different configuration. The default accepts all changes.
pub trait InMemoryState<Config>: Sized {
    type Counter: Counter;
    type Error;

    fn new(config: &Config, now: Instant) -> Self;

    fn try_accept(
        &mut self,
        now: Instant,
        config: &Config,
    ) -> Result<Decision<Self::Counter>, Self::Error>;

    fn validate_config_change(_current: &Config, _next: &Config) -> Result<(), Self::Error> {
        Ok(())
    }
}

impl<T, S> InMemoryState<WindowConfig<T>> for S
where
    T: Counter,
    S: State<T>,
{
    type Counter = T;
    type Error = <S as State<T>>::Error;

    fn new(_config: &WindowConfig<T>, now: Instant) -> Self {
        <S as State<T>>::new(now)
    }

    fn try_accept(
        &mut self,
        now: Instant,
        config: &WindowConfig<T>,
    ) -> Result<Decision<T>, Self::Error> {
        <S as State<T>>::try_accept(self, now, config.limit(), config.window())
    }

    fn validate_config_change(
        current: &WindowConfig<T>,
        next: &WindowConfig<T>,
    ) -> Result<(), Self::Error> {
        <S as State<T>>::validate_config_change(current, next)
    }
}
