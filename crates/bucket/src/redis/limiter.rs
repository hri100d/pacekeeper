use std::time::Duration;

use pacekeeper_core::Decision;

use crate::{BucketConfig, BucketCounter, BucketError};

use super::{commands::RedisCommandFactory, connection::RedisConnection};

const REDIS_KEY_PREFIX: &str = "pacekeeper:bucket:v1";

pub struct RedisTokenBucket<C: BucketCounter = u64, R: BucketCounter = u64> {
    connection: RedisConnection,
    commands: RedisCommandFactory<C, R>,
    prefix: String,
}

impl<C: BucketCounter, R: BucketCounter> RedisTokenBucket<C, R> {
    pub async fn connect(
        url: &str,
        namespace: &str,
        config: BucketConfig<C, R>,
    ) -> Result<Self, BucketError> {
        let client = ::redis::Client::open(url)?;

        let connection = client.get_connection_manager().await?;

        Ok(Self::from_connection(
            RedisConnection::Standalone(connection),
            namespace,
            config,
        ))
    }

    #[cfg(feature = "redis-cluster")]
    pub async fn connect_cluster(
        seed_urls: Vec<String>,
        namespace: &str,
        config: BucketConfig<C, R>,
    ) -> Result<Self, BucketError> {
        let client = ::redis::cluster::ClusterClient::new(seed_urls)?;

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

    pub fn config(&self) -> BucketConfig<C, R> {
        self.commands.config()
    }

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
