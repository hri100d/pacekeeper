use std::time::Duration;

use pacekeeper_core::Decision;

use crate::{BucketConfig, BucketCounter, BucketError};

use super::{commands::RedisCommandFactory, connection::RedisConnection};

const REDIS_KEY_PREFIX: &str = "pacekeeper:bucket:v1";

/// An asynchronous Redis-backed token bucket.
///
/// Available with `redis` or `redis-cluster`.
///
/// Instances using the same Redis deployment, namespace, and key share
/// usage. Their configurations must match while the bucket key exists.
///
/// Acquisition uses Redis server time and an atomic Lua script.
/// Redis errors are returned without automatic fallback to another backend.
///
/// Create the limiter once and reuse its connection.
pub struct RedisTokenBucket<C: BucketCounter = u64, R: BucketCounter = u64> {
    connection: RedisConnection,
    commands: RedisCommandFactory<C, R>,
    prefix: String,
}

impl<C: BucketCounter, R: BucketCounter> RedisTokenBucket<C, R> {
    /// Connects to standalone Redis using default connection options.
    ///
    /// Available with the `redis` feature. Requires a Tokio runtime.
    ///
    /// `namespace` separates this policy's keys from other policies.
    /// The connection is established before this method returns.
    ///
    /// # Errors
    ///
    /// Returns an error if the URL is invalid or the connection cannot be
    /// established.
    #[cfg(feature = "redis")]
    pub async fn connect(
        url: &str,
        namespace: &str,
        config: BucketConfig<C, R>,
    ) -> Result<Self, BucketError> {
        Self::connect_with_options(url, namespace, config, RedisOptions::default()).await
    }

    /// Connects to an existing Redis Cluster using default connection options.
    ///
    /// Available with the `redis-cluster` feature. Requires a Tokio runtime.
    ///
    /// Seed URLs identify initial nodes used to discover the cluster.
    /// Discovered node addresses must be reachable by the application.
    /// This method does not create or configure a Redis Cluster.
    ///
    /// # Errors
    ///
    /// Returns an error if the seed configuration is invalid or the cluster
    /// connection cannot be established.
    #[cfg(feature = "redis-cluster")]
    pub async fn connect_cluster(
        seed_urls: Vec<String>,
        namespace: &str,
        config: BucketConfig<C, R>,
    ) -> Result<Self, BucketError> {
        Self::connect_cluster_with_options(seed_urls, namespace, config, RedisOptions::default())
            .await
    }

    /// Connects to standalone Redis with explicit timeout options.
    ///
    /// Available with the `redis` feature. Requires a Tokio runtime.
    ///
    /// # Errors
    ///
    /// Returns an error if the URL is invalid or the connection cannot be
    /// established.
    #[cfg(feature = "redis")]
    pub async fn connect_with_options(
        url: &str,
        namespace: &str,
        config: BucketConfig<C, R>,
        options: RedisOptions,
    ) -> Result<Self, BucketError> {
        let client = ::redis::Client::open(url)?;

        let manager_config = ::redis::aio::ConnectionManagerConfig::new()
            .set_connection_timeout(Some(options.connection_timeout))
            .set_response_timeout(Some(options.response_timeout));

        let connection = client
            .get_connection_manager_with_config(manager_config)
            .await?;

        Ok(Self::from_connection(
            RedisConnection::Standalone(connection),
            namespace,
            config,
        ))
    }

    /// Connects to an existing Redis Cluster with explicit timeout options.
    ///
    /// Available with the `redis-cluster` feature. Requires a Tokio runtime.
    ///
    /// # Errors
    ///
    /// Returns an error if the seed configuration is invalid or the cluster
    /// connection cannot be established.
    #[cfg(feature = "redis-cluster")]
    pub async fn connect_cluster_with_options(
        seed_urls: Vec<String>,
        namespace: &str,
        config: BucketConfig<C, R>,
        options: RedisOptions,
    ) -> Result<Self, BucketError> {
        let client = ::redis::cluster::ClusterClient::builder(seed_urls)
            .connection_timeout(options.connection_timeout)
            .response_timeout(options.response_timeout)
            .build()?;

        let connection = client.get_async_connection().await?;

        Ok(Self::from_connection(
            RedisConnection::Cluster(connection),
            namespace,
            config,
        ))
    }

    fn from_connection(
        connection: RedisConnection,
        namespace: &str,
        config: BucketConfig<C, R>,
    ) -> Self {
        Self {
            connection,
            commands: RedisCommandFactory::new(config),
            prefix: format!("{REDIS_KEY_PREFIX}:{}:{namespace}:", namespace.len(),),
        }
    }

    /// Atomically attempts to consume one token for a key.
    ///
    /// A missing bucket starts full. Refill, admission, state updates, and
    /// expiration are handled by the Lua script.
    ///
    /// An allowed decision's `reset_after` is the time until full capacity.
    /// A denied decision's `retry_after` is the time until one token is available.
    ///
    /// The script is invoked with `EVALSHA`, falling back to `EVAL` when the
    /// owning Redis node does not have it cached.
    ///
    /// # Errors
    ///
    /// Returns an error for configuration mismatch, invalid stored state,
    /// backwards server time, an unexpected response, or a Redis failure.
    ///
    /// A connection error or timeout does not prove the token was not consumed.
    /// Retrying may consume another token.
    pub async fn try_acquire(&self, key: &str) -> Result<Decision<C>, BucketError> {
        let redis_key = self.redis_key(key);
        let command = self.commands.evalsha(&redis_key);

        let response = match self.connection.query::<(u8, u64, u64)>(&command).await {
            Ok(response) => response,

            Err(error)
                if error.kind()
                    == ::redis::ErrorKind::Server(::redis::ServerErrorKind::NoScript) =>
            {
                let command = self.commands.eval(&redis_key);
                self.connection.query(&command).await?
            }

            Err(error) => return Err(error.into()),
        };

        let (status, remaining, wait_us) = response;
        let wait = Duration::from_micros(wait_us);

        match status {
            1 => {
                let capacity: u64 = self.config().capacity().into();

                if remaining >= capacity {
                    return Err(BucketError::InvalidRedisResponse);
                }

                let remaining = C::from(remaining).ok_or(BucketError::InvalidRedisResponse)?;

                Ok(Decision::Allowed {
                    remaining,
                    reset_after: wait,
                })
            }

            0 => {
                if remaining != 0 || wait_us == 0 {
                    return Err(BucketError::InvalidRedisResponse);
                }

                Ok(Decision::Denied { retry_after: wait })
            }

            _ => Err(BucketError::InvalidRedisResponse),
        }
    }

    /// Returns the configuration supplied when this limiter was created.
    ///
    /// This reads local configuration and does not query Redis.
    pub fn config(&self) -> BucketConfig<C, R> {
        self.commands.config()
    }

    /// Deletes the shared bucket for this namespace and key.
    ///
    /// Returns whether a key was deleted. The next acquisition starts from a
    /// full bucket. Removal affects all instances sharing this bucket.
    ///
    /// # Errors
    ///
    /// Returns an error if the Redis operation fails.
    pub async fn remove_key(&self, key: &str) -> Result<bool, BucketError> {
        let redis_key = self.redis_key(key);
        let command = self.commands.delete(&redis_key);

        let deleted: u64 = self.connection.query(&command).await?;

        Ok(deleted != 0)
    }

    fn redis_key(&self, key: &str) -> String {
        format!("{}{key}", self.prefix)
    }
}

/// Timeout settings passed to the Redis client.
///
/// Both values default to five seconds.
/// These configure client operations, not a guaranteed total deadline
/// across every retry or connection attempt.
#[derive(Debug, Clone, Copy)]
pub struct RedisOptions {
    /// Timeout configured for establishing a connection.
    pub connection_timeout: Duration,

    /// Timeout configured for receiving a command response.
    pub response_timeout: Duration,
}

impl Default for RedisOptions {
    fn default() -> Self {
        Self {
            connection_timeout: Duration::from_secs(5),
            response_timeout: Duration::from_secs(5),
        }
    }
}
