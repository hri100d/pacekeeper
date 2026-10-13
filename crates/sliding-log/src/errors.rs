use std::{error::Error, fmt};

use pacekeeper_core::RateLimiterError;

#[derive(Debug)]
pub enum WindowStateError {
    CounterOverflow,
    WindowChangeNotAllowed,
}

impl fmt::Display for WindowStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CounterOverflow => write!(
                f,
                "request count cannot be represented by the target integer type"
            ),
            Self::WindowChangeNotAllowed => {
                write!(f, "the sliding-log window duration cannot be changed")
            }
        }
    }
}

impl Error for WindowStateError {}

pub type SlidingLogWindowError = RateLimiterError<WindowStateError>;
