use std::hash::Hash;

pub trait Key: Eq + Hash {}

impl<T: Eq + Hash + ?Sized> Key for T {}

pub trait LookupKey<K: Key>: Key + ToOwned<Owned = K> {}

impl<K, Q> LookupKey<K> for Q
where
    K: Key,
    Q: Key + ToOwned<Owned = K> + ?Sized,
{
}
