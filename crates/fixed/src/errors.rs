use std::{error::Error, fmt};

use pacekeeper_core::RateLimiterError;

#[derive(Debug)]
pub enum WindowStateError {
    DurationOverflow,
}

impl fmt::Display for WindowStateError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::DurationOverflow => {
                write!(f, "elapsed time exceeds the fixed window duration")
            }
        }
    }
}

impl Error for WindowStateError {}

pub type FixedWindowError = RateLimiterError<WindowStateError>;
