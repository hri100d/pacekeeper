use std::hash::Hash;

/// Equality and hashing requirements for a limiter key.
///
/// Implemented automatically for types satisfying `Eq + Hash`.
/// A key's equality and hash must remain stable while it is stored.
pub trait Key: Eq + Hash {}

impl<T: Eq + Hash + ?Sized> Key for T {}

/// A lookup key that can produce the owned key stored by the limiter.
///
/// For example, `str` can look up a stored `String` and create one through
/// `ToOwned` when the key is first inserted.
///
/// The borrowed lookup's equality and hashing must agree with the owned
/// key's equality and hashing.
pub trait LookupKey<K: Key>: Key + ToOwned<Owned = K> {}

impl<K, Q> LookupKey<K> for Q
where
    K: Key,
    Q: Key + ToOwned<Owned = K> + ?Sized,
{
}
