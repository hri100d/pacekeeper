# pacekeeper-sliding-log

An in-memory sliding-window log rate limiter.

The limiter records the timestamp of every accepted request and expires
requests individually as they leave the rolling window.

## Installation

```toml
[dependencies]
pacekeeper-sliding-log = "0.2"
pacekeeper-core = "0.2"
```

Alternatively, enable `sliding-log` on the `pacekeeper` crate.

## Example

```rust
use std::time::Duration;

use pacekeeper_core::Decision;
use pacekeeper_sliding_log::SlidingLogRateLimiter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limiter = SlidingLogRateLimiter::<String>::new(
        100,
        Duration::from_secs(60),
    )?;

    match limiter.try_acquire("user:42")? {
        Decision::Allowed {
            remaining,
            reset_after,
        } => {
            println!(
                "{remaining} requests remain; \
                 the oldest request expires in {reset_after:?}"
            );
        }
        Decision::Denied { retry_after } => {
            println!("Retry in {retry_after:?}");
        }
    }

    Ok(())
}
```

## Rolling-window behavior

For each acquisition, timestamps whose age is at least the configured
window duration are removed.

The request is allowed only if the number of retained accepted requests
is below the limit. Denied requests are not appended to the log.

With a limit of three requests per minute, requests accepted at seconds
0, 10, and 20 expire at seconds 60, 70, and 80 respectively.

`reset_after` is the time until the oldest retained request expires.
It is not the time until the entire allowance is restored.

`retry_after` is the time until enough requests expire to admit one more.

## Configuration

The limit and window duration are fixed at construction. `config()` returns
a configuration value directly without acquiring the state mutex.

Create a new limiter to change the policy. The new instance starts with
empty usage state; existing history is not transferred.

See the [workspace changelog](https://github.com/hri100d/pacekeeper/blob/main/CHANGELOG.md)
for version history and migration instructions.

## Memory and cleanup

Existing constructors use one shard. To select more, pass a validated
`WindowConfig` to `SlidingLogRateLimiter::new_with_config_and_shards(config, 8)`.
The count must be a nonzero power of two; otherwise construction returns
`pacekeeper_core::InvalidShardCount`. Different shards can process requests
concurrently, while keys in the same shard share a mutex.

Each key stores a queue of accepted-request timestamps. Memory use grows
with retained history, so this algorithm uses more memory than a fixed
counter.

Expired timestamps are removed when that key is accessed. There is no
background cleanup task, and idle key entries remain in the map.

Call `remove_key` to forget a key and its complete history.

## Keys, counters, and time

The default key and counter types are `String` and `u64`.

Borrowed lookups are supported, including `&str` for stored `String` keys.

`try_acquire_at` supports explicit timestamps. Calls for a key must be
processed with nondecreasing timestamps because the log relies on
chronological ordering.

Use `Arc` to share an instance between threads.

## Errors

Construction rejects zero limits and zero windows.

Acquisition can report a poisoned lock or a counter conversion failure.
Removal can report a poisoned lock. Configuration reads do not return errors.

## License

Licensed under the [MIT License](LICENSE).
