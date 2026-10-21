# pacekeeper-core

Shared types, traits, and in-memory engines for Pacekeeper.

Most applications should start with the `pacekeeper` crate or one of its
algorithm crates. Use `pacekeeper-core` directly when you need shared
configuration types, generic limiter interfaces, or custom state
implementations.

## Installation

```toml
[dependencies]
pacekeeper-core = "0.2"
```

This crate does not provide a complete rate-limiting algorithm by itself.

## Shared configuration

`WindowConfig<T>` stores a positive request limit and a nonzero window
duration.

```rust
use std::time::Duration;
use pacekeeper_core::WindowConfig;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = WindowConfig::new(
        100u32,
        Duration::from_secs(60),
    )?;

    assert_eq!(config.limit(), 100);
    assert_eq!(config.window(), Duration::from_secs(60));

    Ok(())
}
```

## Public types

| Type                  | Purpose                                                 |
| --------------------- | ------------------------------------------------------- |
| `WindowConfig<T>`     | Validated window configuration                          |
| `Decision<T>`         | Allowed or denied acquisition result                    |
| `RateLimiter<Q, T>`   | Synchronous acquisition and key-removal interface       |
| `Counter`             | Unsigned integer counter requirements                   |
| `Key`                 | Equality and hashing requirements                       |
| `LookupKey<K>`        | Borrowed lookup that can produce an owned key           |
| `RateLimiterError<E>` | Window-limiter configuration, locking, and state errors |
| `InMemoryError<E>`    | Generic in-memory engine errors                         |
| `InvalidShardCount`   | Zero or non-power-of-two shard count                    |

`Counter` has a blanket implementation for types satisfying its unsigned
integer bounds.

`Key` has a blanket implementation for types implementing `Eq + Hash`.

A `String` key can be looked up using `str`. The lookup's equality and
hashing must agree with the owned key.

## Synchronous limiter interface

```rust
use pacekeeper_core::{Decision, RateLimiter};

fn acquire_for_user<L>(
    limiter: &L,
    user: &str,
) -> Result<Decision<u64>, L::Error>
where
    L: RateLimiter<str, u64> + ?Sized,
{
    limiter.try_acquire(user)
}
```

The asynchronous Redis token bucket does not implement this synchronous
trait.

## Custom algorithms

The crate exposes two engines:

- `InMemoryLimiter<K, Config, S>` accepts a state implementation of
  `InMemoryState<Config>`.
- `InMemoryWindowLimiter<K, T, S>` specializes the engine for
  `WindowConfig<T>` and `State<T>`.

Types implementing `State<T>` automatically receive an
`InMemoryState<WindowConfig<T>>` implementation.

State construction should initialize the algorithm without consuming the
incoming request. The engine calls `try_accept` once after insertion.

Existing constructors use one shard. Both engines also expose
`new_with_config_and_shards(config, shard_count)`, which accepts a nonzero
power of two and otherwise returns `InvalidShardCount`.
Each shard owns a hash map and mutex. Acquisitions and removals on different
shards can run concurrently; operations within one shard are serialized.
The engine reads its automatic clock after acquiring the key's shard lock.
A poisoned shard returns the existing `StateUnavailable` error for its keys;
other shards remain usable.

Configuration is fixed at construction and stored outside the state mutex.
`config()` clones the configuration without taking that lock, even if it is
poisoned. Only this accessor requires `Config: Clone`; state operations do not.

The constructor takes an owned configuration and stores it directly, without
an internal `Arc` or a separate allocation for the configuration wrapper.
Acquisitions borrow that stored value; they do not clone it. Share an entire
limiter using `Arc<Limiter>` to share both policy and usage across threads.
For the generic engine, sharing across threads requires `Config: Send + Sync`
in addition to the appropriate bounds on keys and state.

Constructing a replacement engine starts with empty state.

See the [workspace changelog](https://github.com/hri100d/pacekeeper/blob/main/CHANGELOG.md)
for version history and migration instructions.

## State lifetime

The engines retain key entries until explicit removal or until the engine
is dropped. They do not run a background cleanup task.

Explicit timestamps should be nondecreasing for each key in processing
order. Algorithm implementations determine how invalid timestamps are
handled.

The meaning of `Decision::Allowed::reset_after` is algorithm-specific.

## License

Licensed under the [MIT License](LICENSE).
