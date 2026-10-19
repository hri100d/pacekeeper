use std::time::{Duration, Instant};

use crate::{Counter, Decision, WindowConfig};

/// Per-key state for a window-based rate-limiting algorithm.
///
/// `new` initializes state without consuming a request.
/// The engine subsequently calls `try_accept` for the incoming request.
///
/// The built-in in-memory engine uses a fixed configuration for its lifetime.
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
}

/// Per-key state used by the generic in-memory engine.
///
/// `new` initializes state without consuming a request.
/// `try_accept` evaluates and records one acquisition.
///
/// Methods run while the engine's mutex is held. Avoid blocking I/O or
/// re-entering the same limiter from these methods.
///
/// The built-in engine does not replace its configuration after construction.
pub trait InMemoryState<Config>: Sized {
    type Counter: Counter;
    type Error;

    fn new(config: &Config, now: Instant) -> Self;

    fn try_accept(
        &mut self,
        now: Instant,
        config: &Config,
    ) -> Result<Decision<Self::Counter>, Self::Error>;
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
}
