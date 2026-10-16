# Pacekeeper

Configurable rate limiting for Rust applications.

Pacekeeper provides fixed-window, sliding-window log, sliding-window counter,
and token-bucket limiters. Enable the algorithms and backends your application
needs through Cargo features.

Requests are tracked by a caller-supplied key, such as a user ID, API key,
IP address, or group ID. Each successful acquisition consumes one request
allowance or one token.

## Installation

Enable one limiter:

```toml
[dependencies]
pacekeeper = { version = "0.1", features = ["fixed"] }
```

Enable multiple limiters:

```toml
[dependencies]
pacekeeper = { version = "0.1", features = ["fixed", "bucket"] }
```

Enable every implementation:

```toml
[dependencies]
pacekeeper = { version = "0.1", features = ["all"] }
```

No limiter features are enabled by default.

## Features

| Feature                | Implementation                          |
| ---------------------- | --------------------------------------- |
| `fixed`                | In-memory fixed-window counter          |
| `sliding-log`          | In-memory sliding-window log            |
| `sliding-counter`      | In-memory sliding-window counter        |
| `bucket`               | In-memory token bucket                  |
| `bucket-redis`         | Token bucket backed by standalone Redis |
| `bucket-redis-cluster` | Token bucket backed by Redis Cluster    |
| `all`                  | All implementations and backends        |

Redis features use asynchronous APIs with Tokio.

Enabling `bucket-redis-cluster` alone does not enable the standalone Redis
backend or the in-memory token bucket.

## Quick start

```rust
use std::time::Duration;

use pacekeeper::{
    common::Decision,
    fixed::FixedRateLimiter,
};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limiter = FixedRateLimiter::<String>::new(
        100,
        Duration::from_secs(60),
    )?;

    match limiter.try_acquire("user:42")? {
        Decision::Allowed {
            remaining,
            reset_after,
        } => {
            println!(
                "Allowed: {remaining} requests remain; \
                 window ends in {reset_after:?}"
            );
        }
        Decision::Denied { retry_after } => {
            println!("Denied: retry in {retry_after:?}");
        }
    }

    Ok(())
}
```

Create the limiter once and reuse it across requests. Creating a new
in-memory limiter for every request also creates fresh usage state.

Use `Arc` to share an in-memory limiter between threads. Its internal mutex
protects configuration and per-key state.

## Choosing an algorithm

| Algorithm       | Behavior                                           | State per tracked key               |
| --------------- | -------------------------------------------------- | ----------------------------------- |
| Fixed window    | Counts accepted requests in one window             | Constant-sized                      |
| Sliding log     | Tracks individual accepted request timestamps      | Grows with retained request history |
| Sliding counter | Estimates usage using current and previous windows | Constant-sized                      |
| Token bucket    | Allows bursts up to capacity and refills over time | Constant-sized                      |

Fixed windows can allow bursts across a window boundary.

The sliding log enforces an exact rolling window when timestamps are
nondecreasing.

The sliding counter approximates a rolling window. It uses integer
arithmetic and rounds the previous window's weighted contribution upward.

The token bucket supports an initial burst up to its capacity and a
configurable sustained refill rate.

## Understanding decisions

An allowed decision means one request has already been counted or one
token has already been consumed.

`remaining` is the number of additional whole requests or tokens available
at the evaluated time. For the sliding counter, it is based on estimated
usage.

`reset_after` depends on the algorithm:

| Algorithm       | Meaning of `reset_after`                                |
| --------------- | ------------------------------------------------------- |
| Fixed window    | Time until the current window expires                   |
| Sliding log     | Time until the oldest retained accepted request expires |
| Sliding counter | Time until the current counter window ends              |
| Token bucket    | Time until the bucket becomes completely full           |

A denied decision contains `retry_after`: the calculated wait until another
request can be admitted, assuming unchanged configuration and no intervening
accepted requests.

A returned duration does not reserve future capacity.

## Keys and cleanup

In-memory limiters support custom owned key types and borrowed lookups.
For example, a limiter storing `String` keys can be queried using `&str`.

Redis buckets use string keys.

In-memory key entries are retained until `remove_key` is called or the
limiter is dropped. Expiration of usage does not automatically remove the
key from the map.

Removing a key forgets its usage. Its next request starts with fresh state.

Redis token-bucket keys expire when the bucket would be full, with a small
expiration margin. A missing key represents a full bucket.

## Configuration and time

Fixed-window limiters support changing the limit and window duration
without clearing existing state.

Sliding-log and sliding-counter limiters support changing the limit, but
reject changes to the window duration.

Token-bucket configuration is fixed for the lifetime of the public limiter
instance.

In-memory APIs provide `try_acquire_at` for explicit timestamps. Supply
nondecreasing timestamps for each key in the order calls are processed.

Redis buckets use the Redis server's clock and do not expose
`try_acquire_at`.

## Redis behavior

Redis token acquisition runs inside a Lua script so that reading,
refilling, deciding, and updating one bucket happen atomically.

Instances using the same Redis deployment, namespace, and key share usage.
They must use matching bucket configurations while that key exists.

Redis errors are returned to the application. Automatic fail-open,
fail-closed, and in-memory fallback policies are not implemented.

See the [bucket README](crates/bucket/README.md) for backend examples,
configuration limits, and Redis behavior.

## Workspace

| Crate                        | Purpose                                     |
| ---------------------------- | ------------------------------------------- |
| `pacekeeper`                 | Feature-based entry point                   |
| `pacekeeper-core`            | Shared types, traits, and in-memory engines |
| `pacekeeper-fixed`           | Fixed-window limiter                        |
| `pacekeeper-sliding-log`     | Sliding-window log limiter                  |
| `pacekeeper-sliding-counter` | Sliding-window counter limiter              |
| `pacekeeper-bucket`          | In-memory and Redis token buckets           |

Each implementation crate can also be used directly.

## Development

Run ordinary tests:

```sh
cargo test --workspace --all-features
```

Run Redis integration tests with Docker running:

```sh
cargo test -p pacekeeper-bucket \
  --all-features \
  --test redis_docker \
  -- --ignored --nocapture
```

The Redis tests create and remove their own containers. The cluster fixture
runs three primary Redis processes in one container. It does not test
replica failover.

Check formatting, linting, and documentation:

```sh
cargo fmt --all --check
cargo clippy --workspace --all-targets --all-features -- -D warnings
cargo doc --workspace --all-features --no-deps
```

## License

Licensed under the [MIT License](LICENSE).
