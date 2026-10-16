use num_traits::{PrimInt, Unsigned};

/// An unsigned integer usable as a request counter.
///
/// Implemented automatically for types satisfying `PrimInt + Unsigned`.
/// Individual algorithms may impose additional arithmetic limits.
pub trait Counter: PrimInt + Unsigned {}

impl<T: PrimInt + Unsigned> Counter for T {}
