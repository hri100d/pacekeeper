# Changelog

Notable changes across the Pacekeeper workspace are recorded here, newest
first.

## 0.2.0

This release includes breaking API changes from 0.1.0. See the migration
instructions below before upgrading.

### Added

- Optional sharding for all in-memory limiters and the core engines through
  `new_with_config_and_shards(config, shard_count)`. Counts must be nonzero
  powers of two; invalid counts return the public `InvalidShardCount` error.
- Private storage built from standard-library hash maps and mutexes, with
  independent locks per shard and no additional dependencies.

### Changed

- In-memory configuration is fixed at construction and stored directly in
  the engine, outside the mutex protecting per-key state. Acquisitions borrow
  the configuration; they do not clone it.
- In-memory `config()` returns an owned configuration value directly instead
  of a `Result`. Reads no longer acquire the state mutex and remain available
  if that mutex is poisoned. Only this accessor requires `Config: Clone` on
  the generic engine.
- Sharing a generic engine between threads now requires `Config: Sync` as
  well as `Send`, because the configuration is no longer behind a mutex.
  The built-in configuration types satisfy these bounds.

### Removed

- `update_config` from the fixed-window, sliding-log, and sliding-counter
  limiters and the core in-memory engines.
- `validate_config_change` from the `State` and `InMemoryState` traits,
  including the built-in implementations and the blanket adapter.
- `WindowChangeNotAllowed` from the sliding-log and sliding-counter
  `WindowStateError` enums, since those limiters no longer change configuration.

### Migration from 0.1.0

- Replace `limiter.config()?` or `limiter.config().unwrap()` with
  `limiter.config()` for in-memory limiters, including `TokenBucket`.
- Construct a new limiter to use a different policy. Its in-memory usage
  starts empty; existing counters and history are not transferred. Reuse
  the instance across requests rather than constructing one per request.
- Remove `validate_config_change` methods from custom `State` and
  `InMemoryState` trait implementations. The methods are no longer members
  of those traits.
- Remove match arms referencing the deleted `WindowChangeNotAllowed` variants.
- The Redis configuration accessor and policy-mismatch behavior are
  unchanged. Redis instances accessing an existing bucket must still use
  matching configuration.

Existing constructors retain one shard. Opt into multiple shards with the
new constructor; the count cannot change during the limiter's lifetime.
Snapshot recovery is not part of these changes.

## 0.1.0

Initial release of the Pacekeeper workspace:

- Fixed-window, sliding-window log, and sliding-window counter limiters.
- Token buckets with optional in-memory, standalone Redis, and Redis Cluster
  backends.
- Shared configuration, decision types, traits, and in-memory engines.
- Cargo features for selecting algorithms and bucket backends.

[Release tag](https://github.com/hri100d/pacekeeper/releases/tag/v0.1.0)
