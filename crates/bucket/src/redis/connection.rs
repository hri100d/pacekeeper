use ::redis::{Cmd, FromRedisValue, RedisResult};

#[cfg(feature = "redis")]
use ::redis::aio::ConnectionManager;

#[cfg(feature = "redis-cluster")]
use ::redis::cluster_async::ClusterConnection;

pub(super) enum RedisConnection {
    #[cfg(feature = "redis")]
    Standalone(ConnectionManager),

    #[cfg(feature = "redis-cluster")]
    Cluster(ClusterConnection),
}

impl RedisConnection {
    pub(super) async fn query<T: FromRedisValue>(&self, command: &Cmd) -> RedisResult<T> {
        match self {
            #[cfg(feature = "redis")]
            Self::Standalone(connection) => command.query_async(&mut connection.clone()).await,

            #[cfg(feature = "redis-cluster")]
            Self::Cluster(connection) => command.query_async(&mut connection.clone()).await,
        }
    }
}
