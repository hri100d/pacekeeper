use std::time::Duration;

use crate::Counter;

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Decision<T: Counter = u64> {
    Allowed { remaining: T, reset_after: Duration },
    Denied { retry_after: Duration },
}
