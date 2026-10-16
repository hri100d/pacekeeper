# pacekeeper-fixed

An in-memory fixed-window rate limiter.

Each key has its own window start and accepted-request counter.

## Installation

```toml
[dependencies]
pacekeeper-fixed = "0.1"
pacekeeper-core = "0.1"
```

Alternatively, enable `fixed` on the `pacekeeper` crate.

## Example

```rust
use std::time::Duration;

use pacekeeper_core::Decision;
use pacekeeper_fixed::FixedRateLimiter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limiter = FixedRateLimiter::<String>::new(
        100,
        Duration::from_secs(60),
    )?;

    match limiter.try_acquire("user:42")? {
        Decision::Allowed { remaining, .. } => {
            println!("Allowed; {remaining} requests remain");
        }
        Decision::Denied { retry_after } => {
            println!("Retry in {retry_after:?}");
        }
    }

    Ok(())
}
```

## Window behavior

The first acquisition for a new key starts its window.

Once that window has expired, the next acquisition starts a new window at
the time of that request. Windows are not aligned to wall-clock minutes.

Allowed requests increment the counter. Denied requests do not consume
additional allowance.

Both `reset_after` and `retry_after` indicate the time until the current
window expires.

A fixed window can allow a burst near the end of one window followed by
another burst in the next window.

## Configuration updates

```rust
use std::time::Duration;

use pacekeeper_core::WindowConfig;
use pacekeeper_fixed::FixedRateLimiter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limiter = FixedRateLimiter::<String>::new(
        100,
        Duration::from_secs(60),
    )?;

    limiter.update_config(WindowConfig::new(
        200,
        Duration::from_secs(60),
    )?)?;

    Ok(())
}
```

Both the limit and window duration can change.

Updating configuration preserves existing counters and window starts.
Subsequent acquisitions evaluate that state using the new configuration.
Lowering a limit does not erase already accepted requests.

## Keys and concurrency

The default key and counter types are `String` and `u64`.

Custom types can be selected explicitly:

```rust
use std::time::Duration;
use pacekeeper_fixed::FixedRateLimiter;

let limiter = FixedRateLimiter::<u64, u32>::new(
    100,
    Duration::from_secs(60),
)
.unwrap();

let decision = limiter.try_acquire(&42).unwrap();
```

Use `Arc` to share one instance between threads. Separate instances have
independent state.

## Explicit time and cleanup

`try_acquire_at` accepts an `Instant` for deterministic tests or externally
managed monotonic time. Supply nondecreasing timestamps for each key.

`remove_key` returns whether a stored entry existed. Removing it resets
that key's usage.

Idle key entries are not automatically removed.

## Errors

Construction rejects a zero limit or zero window.

Operations can return a poisoned-lock error or an algorithm state error
through `FixedWindowError`.

## License

Licensed under the [MIT License](LICENSE).
