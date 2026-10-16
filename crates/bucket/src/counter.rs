use pacekeeper_core::Counter;

mod sealed {
    pub trait Sealed {}
}

/// An unsigned counter supported by token-bucket implementations.
///
/// Implemented only for `u8`, `u16`, `u32`, and `u64`.
/// This trait is sealed and cannot be implemented outside this crate.
///
/// A supported integer type does not make every configuration valid;
/// `BucketConfig::new` also checks arithmetic bounds.
pub trait BucketCounter: Counter + Into<u64> + sealed::Sealed {}

macro_rules! impl_bucket_counter {
    ($($ty:ty),+ $(,)?) => {
        $(
            impl sealed::Sealed for $ty {}
            impl BucketCounter for $ty {}
        )+
    };
}

impl_bucket_counter!(u8, u16, u32, u64);
