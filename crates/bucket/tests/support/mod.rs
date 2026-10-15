use std::time::Duration;

use pacekeeper_bucket::{BucketConfig, RedisTokenBucket};
use testcontainers::{
    GenericImage, ImageExt,
    core::{ContainerAsync, IntoContainerPort, WaitFor},
    runners::AsyncRunner,
};

pub type TestResult<T = ()> = Result<T, Box<dyn std::error::Error + Send + Sync>>;

enum Backend {
    #[cfg(feature = "redis")]
    Standalone,

    #[cfg(feature = "redis-cluster")]
    Cluster,
}

pub struct RedisFixture {
    pub container: ContainerAsync<GenericImage>,
    pub urls: Vec<String>,
    backend: Backend,
}

impl RedisFixture {
    #[cfg(feature = "redis")]
    pub async fn standalone() -> TestResult<Self> {
        let container = GenericImage::new("redis", "7.4-alpine")
            .with_exposed_port(6379.tcp())
            .with_wait_for(WaitFor::message_on_stdout("Ready to accept connections"))
            .with_cmd(["redis-server", "--save", "", "--appendonly", "no"])
            .with_startup_timeout(Duration::from_secs(90))
            .start()
            .await?;

        let host = container.get_host().await?;
        let port = container.get_host_port_ipv4(6379.tcp()).await?;

        Ok(Self {
            container,
            urls: vec![format!("redis://{host}:{port}/")],
            backend: Backend::Standalone,
        })
    }

    #[cfg(feature = "redis-cluster")]
    pub async fn cluster() -> TestResult<Self> {
        use std::net::TcpListener;

        let mut reservations = Vec::new();

        while reservations.len() < 3 {
            let listener = TcpListener::bind(("127.0.0.1", 0))?;

            if listener.local_addr()?.port() >= 20_000 {
                reservations.push(listener);
            }
        }

        let ports = reservations
            .iter()
            .map(|listener| listener.local_addr().map(|address| address.port()))
            .collect::<Result<Vec<_>, _>>()?;

        let mut image = GenericImage::new("redis", "7.4-alpine")
            .with_entrypoint("sh")
            .with_wait_for(WaitFor::message_on_stdout("PACEKEEPER_CLUSTER_READY"));

        for &port in &ports {
            image = image.with_exposed_port(port.tcp());
        }

        let mut command = vec![
            "-c".to_owned(),
            include_str!("start-cluster.sh").to_owned(),
            "pacekeeper-cluster".to_owned(),
        ];

        command.extend(ports.iter().map(u16::to_string));

        let mut request = image
            .with_cmd(command)
            .with_startup_timeout(Duration::from_secs(90));

        for &port in &ports {
            request = request.with_mapped_port(port, port.tcp());
        }

        drop(reservations);

        let container = request.start().await?;

        let host = container.get_host().await?.to_string();

        if host != "127.0.0.1" && host != "localhost" {
            return Err("This cluster fixture requires Docker on the local host".into());
        }

        let urls = ports
            .iter()
            .map(|port| format!("redis://127.0.0.1:{port}/"))
            .collect();

        Ok(Self {
            container,
            urls,
            backend: Backend::Cluster,
        })
    }

    pub async fn connect(
        &self,
        namespace: &str,
        config: BucketConfig,
    ) -> TestResult<RedisTokenBucket> {
        let limiter = match self.backend {
            #[cfg(feature = "redis")]
            Backend::Standalone => {
                RedisTokenBucket::connect(&self.urls[0], namespace, config).await?
            }

            #[cfg(feature = "redis-cluster")]
            Backend::Cluster => {
                RedisTokenBucket::connect_cluster(vec![self.urls[0].clone()], namespace, config)
                    .await?
            }
        };

        Ok(limiter)
    }

    pub async fn query<T: redis::FromRedisValue>(&self, command: &redis::Cmd) -> TestResult<T> {
        match self.backend {
            #[cfg(feature = "redis")]
            Backend::Standalone => {
                let mut connection = redis::Client::open(self.urls[0].as_str())?
                    .get_multiplexed_async_connection()
                    .await?;

                Ok(command.query_async(&mut connection).await?)
            }

            #[cfg(feature = "redis-cluster")]
            Backend::Cluster => {
                let client = redis::cluster::ClusterClient::new(self.urls.clone())?;
                let mut connection = client.get_async_connection().await?;

                Ok(command.query_async(&mut connection).await?)
            }
        }
    }

    pub async fn flush_scripts(&self) -> TestResult {
        for url in &self.urls {
            let mut connection = redis::Client::open(url.as_str())?
                .get_multiplexed_async_connection()
                .await?;

            redis::cmd("SCRIPT")
                .arg("FLUSH")
                .arg("SYNC")
                .query_async::<()>(&mut connection)
                .await?;
        }

        Ok(())
    }

    pub async fn assert_every_node_has_keys(&self) -> TestResult {
        for url in &self.urls {
            let mut connection = redis::Client::open(url.as_str())?
                .get_multiplexed_async_connection()
                .await?;

            let count: u64 = redis::cmd("DBSIZE").query_async(&mut connection).await?;

            assert!(count > 0, "No keys were routed to {url}");
        }

        Ok(())
    }
}
