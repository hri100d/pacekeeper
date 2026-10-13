use std::{error::Error, fmt};

use pacekeeper_core::RateLimiterError;

#[derive(Debug)]
pub enum WindowStateError {
    CounterOverflow,
    WindowChangeNotAllowed,
    DurationOverflow,
    ArithmeticOverflow,
}

impl fmt::Display for WindowStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::CounterOverflow => write!(
                f,
                "request count cannot be represented by the target integer type"
            ),
            Self::ArithmeticOverflow => {
                write!(f, "weighted counter arithmetic exceeded u128")
            }
            Self::DurationOverflow => {
                write!(f, "calculated time cannot be represented")
            }
            Self::WindowChangeNotAllowed => {
                write!(f, "the sliding-counter window duration cannot be changed")
            }
        }
    }
}

impl Error for WindowStateError {}

pub type SlidingCounterWindowError = RateLimiterError<WindowStateError>;
