use num_traits::{PrimInt, Unsigned};

pub trait Counter: PrimInt + Unsigned {}

impl<T: PrimInt + Unsigned> Counter for T {}
