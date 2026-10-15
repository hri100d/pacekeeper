use pacekeeper_core::Counter;

mod sealed {
    pub trait Sealed {}
}

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
