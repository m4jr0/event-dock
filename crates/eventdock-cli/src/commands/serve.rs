use super::AppResult;
use crate::{
    cli::ServeArgs,
    config::{QUERY_QUEUE_CAPACITY, QUERY_WORKERS},
};
use eventdock_engine::{
    Backend, BackendConfig, BackendSnapshot, serve_queries,
};
use std::{
    io,
    net::{SocketAddr, TcpListener},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread::{self, JoinHandle},
    time::Duration,
};

const HEALTH_INTERVAL: Duration = Duration::from_millis(100);

struct Server {
    backend: Arc<Backend>,
    running: Arc<AtomicBool>,
    worker: JoinHandle<io::Result<()>>,
    query_address: SocketAddr,
}

impl Server {
    fn start(
        config: BackendConfig,
        query_address: &str,
    ) -> io::Result<Self> {
        let listener = TcpListener::bind(query_address)?;
        let query_address = listener.local_addr()?;
        let max_query_events = config.max_query_events;
        let backend = Arc::new(Backend::start(config)?);
        let running = Arc::new(AtomicBool::new(true));
        let worker_backend = Arc::clone(&backend);
        let worker_running = Arc::clone(&running);
        let worker = thread::Builder::new()
            .name("eventdock-query-listener".into())
            .spawn(move || {
                serve_queries(
                    listener,
                    worker_backend,
                    worker_running,
                    max_query_events,
                    QUERY_WORKERS,
                    QUERY_QUEUE_CAPACITY,
                )
            });
        match worker {
            Ok(worker) => Ok(Self {
                backend,
                running,
                worker,
                query_address,
            }),
            Err(error) => {
                let cleanup = shutdown_backend(backend);
                combine(
                    Err(error),
                    cleanup.map(|_| unreachable!("startup failed")),
                )
            }
        }
    }

    async fn wait(&self) -> io::Result<()> {
        let signal = tokio::signal::ctrl_c();
        tokio::pin!(signal);
        let mut interval = tokio::time::interval(HEALTH_INTERVAL);
        loop {
            tokio::select! {
                result = &mut signal => return result,
                _ = interval.tick() => self.check_health()?,
            }
        }
    }

    fn check_health(&self) -> io::Result<()> {
        if self.worker.is_finished() {
            return Err(io::Error::other(
                "query listener stopped unexpectedly",
            ));
        }
        if !self.backend.is_running() {
            return Err(io::Error::other(
                "storage worker stopped unexpectedly",
            ));
        }
        if !self.backend.is_ingesting() {
            return Err(io::Error::other(
                "ingestion worker stopped unexpectedly",
            ));
        }
        Ok(())
    }

    fn shutdown(self) -> io::Result<BackendSnapshot> {
        self.running
            .store(false, Ordering::Release);
        let query = self
            .worker
            .join()
            .map_err(|_| io::Error::other("query listener panicked"))
            .and_then(|result| result);
        let backend = shutdown_backend(self.backend);
        combine(query, backend)
    }
}

fn shutdown_backend(backend: Arc<Backend>) -> io::Result<BackendSnapshot> {
    Arc::try_unwrap(backend)
        .map_err(|_| io::Error::other("backend still shared"))?
        .shutdown()
}

// Preserve the primary error and report any additional cleanup failure.
fn combine<T>(
    primary: io::Result<()>,
    cleanup: io::Result<T>,
) -> io::Result<T> {
    match (primary, cleanup) {
        (Ok(()), result) => result,
        (Err(error), Ok(_)) => Err(error),
        (Err(error), Err(cleanup)) => Err(io::Error::new(
            error.kind(),
            format!("{error}; cleanup failed: {cleanup}"),
        )),
    }
}

pub async fn run(args: ServeArgs) -> AppResult<()> {
    let config = BackendConfig {
        address: args.address.parse::<SocketAddr>()?,
        event_capacity: args.capacity,
        ..BackendConfig::default()
    };
    let server = Server::start(config, &args.query_address)?;
    println!("EventDock ingestion: {}", server.backend.address());
    println!("EventDock queries:   {}", server.query_address);

    let outcome = server.wait().await;
    let cleanup = server.shutdown();
    let snapshot = combine(outcome, cleanup)?;
    println!(
        "Stored {} events, rejected {}",
        snapshot.stored_total, snapshot.rejected_total
    );
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn config() -> BackendConfig {
        BackendConfig {
            address: "127.0.0.1:0".parse().unwrap(),
            ..BackendConfig::default()
        }
    }

    #[test]
    fn occupied_query_address_prevents_startup() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let address = listener
            .local_addr()
            .unwrap()
            .to_string();
        let error = Server::start(config(), &address)
            .err()
            .unwrap();
        assert_eq!(error.kind(), io::ErrorKind::AddrInUse);
    }

    #[test]
    fn shutdown_releases_both_listeners() {
        let server = Server::start(config(), "127.0.0.1:0").unwrap();
        let ingestion = server.backend.address();
        let query = server.query_address;
        server.shutdown().unwrap();
        assert!(TcpListener::bind(ingestion).is_ok());
        assert!(TcpListener::bind(query).is_ok());
    }

    #[tokio::test]
    async fn unexpected_query_exit_is_detected() {
        let server = Server::start(config(), "127.0.0.1:0").unwrap();
        server
            .running
            .store(false, Ordering::Release);
        let result =
            tokio::time::timeout(Duration::from_secs(2), server.wait()).await;
        let cleanup = server.shutdown();
        cleanup.unwrap();
        assert!(
            result
                .unwrap()
                .unwrap_err()
                .to_string()
                .contains("query listener")
        );
    }

    #[test]
    fn cleanup_preserves_primary_error() {
        let result = combine::<()>(
            Err(io::Error::new(io::ErrorKind::BrokenPipe, "primary")),
            Err(io::Error::other("secondary")),
        )
        .unwrap_err();
        assert_eq!(result.kind(), io::ErrorKind::BrokenPipe);
        assert!(result.to_string().contains("primary"));
        assert!(result.to_string().contains("secondary"));
    }
}
