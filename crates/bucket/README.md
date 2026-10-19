# pacekeeper-bucket

Token-bucket rate limiting with in-memory, standalone Redis, and
Redis Cluster backends.

Each bucket starts full. An allowed request consumes one token. Tokens
refill continuously at the configured rate, up to the bucket capacity.

## Features

No backend is enabled by default.

| Feature         | Public implementation                                       |
| --------------- | ----------------------------------------------------------- |
| `memory`        | `TokenBucket<K, C, R>`                                      |
| `redis`         | `RedisTokenBucket<C, R>` with standalone connection methods |
| `redis-cluster` | `RedisTokenBucket<C, R>` with cluster connection methods    |

Features can be combined.

The `pacekeeper` entry-point crate exposes these as `bucket`,
`bucket-redis`, and `bucket-redis-cluster`.

## In-memory example

```toml
[dependencies]
pacekeeper-bucket = { version = "0.1", features = ["memory"] }
```

```rust
use std::time::Duration;

use pacekeeper_bucket::{Decision, TokenBucket};

fn main() -> Result<(), Box<dyn std::error::Error>> {
    // Burst capacity: 20 tokens.
    // Sustained refill: 5 tokens per second.
    let limiter = TokenBucket::<String>::new(
        20,
        5,
        Duration::from_secs(1),
    )?;

    match limiter.try_acquire("user:42")? {
        Decision::Allowed {
            remaining,
            reset_after,
        } => {
            println!(
                "{remaining} whole tokens remain; \
                 bucket becomes full in {reset_after:?}"
            );
        }
        Decision::Denied { retry_after } => {
            println!("Another token becomes available in {retry_after:?}");
        }
    }

    Ok(())
}
```

Reuse the instance across requests. Share it with `Arc` when needed.

Each key has an independent bucket. Idle entries are not automatically
removed from the in-memory map.

## Standalone Redis example

```toml
[dependencies]
pacekeeper-bucket = { version = "0.1", features = ["redis"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
use std::time::Duration;

use pacekeeper_bucket::{BucketConfig, RedisTokenBucket};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = BucketConfig::new(
        20u64,
        5u64,
        Duration::from_secs(1),
    )?;

    let limiter = RedisTokenBucket::connect(
        "redis://127.0.0.1:6379/",
        "public-api",
        config,
    )
    .await?;

    let decision = limiter.try_acquire("user:42").await?;
    println!("{decision:?}");

    Ok(())
}
```

The constructor establishes a connection. Reuse the resulting limiter
rather than constructing one for every request.

## Redis Cluster example

```toml
[dependencies]
pacekeeper-bucket = { version = "0.1", features = ["redis-cluster"] }
tokio = { version = "1", features = ["macros", "rt-multi-thread"] }
```

```rust
use std::time::Duration;

use pacekeeper_bucket::{BucketConfig, RedisTokenBucket};

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let config = BucketConfig::new(
        20u64,
        5u64,
        Duration::from_secs(1),
    )?;

    let limiter = RedisTokenBucket::connect_cluster(
        vec![
            "redis://127.0.0.1:7000/".to_owned(),
            "redis://127.0.0.1:7001/".to_owned(),
        ],
        "public-api",
        config,
    )
    .await?;

    let decision = limiter.try_acquire("user:42").await?;
    println!("{decision:?}");

    Ok(())
}
```

Supply addresses of nodes in an already configured Redis Cluster.
The constructor does not create the cluster.

The cluster client discovers nodes and routes each bucket operation to its
owning node. Discovered node addresses must be reachable by the application.

## Configuration

`BucketConfig<C, R>` has three public inputs:

| Input                     | Meaning                                           |
| ------------------------- | ------------------------------------------------- |
| `capacity: C`             | Maximum stored tokens and initial burst allowance |
| `refill_tokens: R`        | Tokens generated per refill period                |
| `refill_period: Duration` | Duration used to express the refill rate          |

Refill is continuous. Five tokens per second means fractional progress
accumulates between requests; it does not wait for a whole second to add
five tokens at once.

`C` and `R` may independently be `u8`, `u16`, `u32`, or `u64`.
The default is `u64` for both.

Configuration validation requires:

- Positive capacity and refill token count.
- A nonzero refill period expressed in whole microseconds.
- A refill period that fits in `u64` microseconds.
- `capacity × refill_period_in_microseconds <= 2^52 - 1`.
- `refill_tokens <= 2^52 - 1`.

These restrictions also apply to the memory backend because it uses the
same configuration type.

Configuration cannot be updated through the public token-bucket APIs.

The in-memory `config()` accessor returns a configuration value without
acquiring the state mutex. Creating a replacement in-memory limiter starts
with full buckets on first access; existing usage is not transferred.

See the [workspace changelog](https://github.com/hri100d/pacekeeper/blob/main/CHANGELOG.md)
for version history and migration instructions.

## Decisions

Every allowed acquisition consumes exactly one token.

`remaining` contains the number of whole tokens left after consumption.

`reset_after` is the time until the bucket becomes full if no additional
tokens are consumed.

`retry_after` is the time until enough credit exists for one token.

Both durations are returned in whole microseconds. Denied requests do not
consume a token, but may update refill accounting.

## Redis state and namespaces

Instances using the same Redis deployment, namespace, and key share usage.

While a bucket key exists, all instances accessing it must use the same
capacity, refill count, and refill period. A mismatch returns
`BucketError::ConfigMismatch`.

Use a distinct namespace for a separate policy. Moving to a new namespace
also starts with fresh usage.

Acquisition executes a Lua script that reads, refills, checks, and updates
one bucket atomically.

The client first uses `EVALSHA`. If that node does not have the script
cached, it falls back to `EVAL`.

Keys expire once the bucket would be completely full, with a small
expiration margin. Recreating a missing full bucket preserves token-bucket
behavior.

## Time

The memory backend uses `Instant` and also provides `try_acquire_at`.

Explicit timestamps must be nondecreasing for each key.
A backwards timestamp returns `BucketError::ClockWentBackwards`.

The Redis backend uses Redis `TIME`. It does not accept application
timestamps and does not provide `try_acquire_at`.

If Redis time is earlier than a bucket's stored timestamp, acquisition
returns `BucketError::ClockWentBackwards`.

## Errors and connection options

`connect_with_options` and `connect_cluster_with_options` accept
`RedisOptions`.

The default connection and response timeouts are both five seconds.

Redis failures are returned as errors. The package does not automatically
allow requests, deny requests, or switch to an in-memory limiter on failure.

A connection error or timeout does not prove that Redis did not execute
the script. Retrying an acquisition can consume another token.

Malformed stored state and unexpected replies are reported through
`BucketError`.

## Removing keys

`remove_key` forgets the bucket and returns whether an entry was removed.
The next acquisition starts from a full bucket.

For Redis, removal affects all instances sharing that namespace and key.

## Testing

Run configuration and memory tests:

```sh
cargo test -p pacekeeper-bucket --features memory
```

Run Redis tests with Docker running:

```sh
cargo test -p pacekeeper-bucket \
  --features redis,redis-cluster \
  --test redis_docker \
  -- --ignored --nocapture
```

Tests create and remove their own Redis containers. The cluster fixture
uses three primaries and does not cover replica failover.

## License

Licensed under the [MIT License](LICENSE).
