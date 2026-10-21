# pacekeeper-sliding-counter

An in-memory sliding-window counter rate limiter.

The limiter approximates rolling-window usage using two counters:
the current window's accepted requests and a weighted contribution from
the previous window.

## Installation

```toml
[dependencies]
pacekeeper-sliding-counter = "0.2"
pacekeeper-core = "0.2"
```

Alternatively, enable `sliding-counter` on the `pacekeeper` crate.

## Example

```rust
use std::time::Duration;

use pacekeeper_core::Decision;
use pacekeeper_sliding_counter::SlidingCounterRateLimiter;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let limiter = SlidingCounterRateLimiter::<String>::new(
        100,
        Duration::from_secs(60),
    )?;

    match limiter.try_acquire("user:42")? {
        Decision::Allowed { remaining, .. } => {
            println!("Estimated remaining allowance: {remaining}");
        }
        Decision::Denied { retry_after } => {
            println!("Retry in {retry_after:?}");
        }
    }

    Ok(())
}
```

## Calculation

Let:

- `W` be the window duration.
- `E` be the elapsed time within the current window.
- `C` be the current window's accepted count.
- `P` be the previous window's accepted count.

The limiter calculates:

```text
weighted_previous = ceil(P × (W - E) / W)
estimated_usage = C + weighted_previous
```

It admits a request only when one additional request fits within the limit.

Calculations use integer arithmetic. Rounding the previous contribution
upward avoids discarding a fractional contribution, but the algorithm
remains an approximation of a timestamp-based sliding log.

For example, with a limit of one, a request accepted at the start of a
window can prevent another request until two complete windows have elapsed.

## Window behavior

Window boundaries remain aligned to the key's initial window start.

After one elapsed window, the current count becomes the previous count.
After two or more elapsed windows, the old history no longer contributes.

`remaining` is the whole-request allowance left under the weighted estimate.

`reset_after` is the time until the current counter window ends.
It does not promise that previous usage stops affecting the next window.

`retry_after` accounts for the decay needed to admit another request and
may extend beyond the current window.

## Configuration

The limit and window duration are fixed at construction. `config()` returns
a configuration value directly without acquiring the state mutex.

Create a new limiter to change the policy. The new instance starts with
empty usage state; existing history is not transferred.

See the [workspace changelog](https://github.com/hri100d/pacekeeper/blob/main/CHANGELOG.md)
for version history and migration instructions.

## Keys, memory, and time

Existing constructors use one shard. To select more, pass a validated
`WindowConfig` to `SlidingCounterRateLimiter::new_with_config_and_shards(config, 8)`.
The count must be a nonzero power of two; otherwise construction returns
`pacekeeper_core::InvalidShardCount`. Different shards can process requests
concurrently, while keys in the same shard share a mutex.

The default key and counter types are `String` and `u64`.

Each tracked key stores constant-sized algorithm state. Key entries are
retained until `remove_key` is called or the limiter is dropped.

Use `Arc` to share one instance between threads.

`try_acquire_at` accepts explicit timestamps. Supply nondecreasing
timestamps for each key in processing order.

## Errors

Construction rejects zero limits and zero windows.

Acquisition can report a poisoned lock, counter conversion failures,
duration conversion failures, or overflow in weighted arithmetic.
Removal can report a poisoned lock. Configuration reads do not return errors.

Large counter values and long durations can exceed the internal `u128`
multiplication range even when each input is individually representable.

## License

Licensed under the [MIT License](LICENSE).
