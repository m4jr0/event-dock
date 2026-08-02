use super::AppResult;
use crate::{
    cli::ServeArgs,
    config::{QUERY_QUEUE_CAPACITY, QUERY_WORKERS},
};
use eventdock_backend::{Backend, BackendConfig, serve_queries};
use std::{
    io,
    net::{SocketAddr, TcpListener},
    sync::{
        Arc,
        atomic::{AtomicBool, Ordering},
    },
    thread,
};

pub async fn run(args: ServeArgs) -> AppResult<()> {
    let config = BackendConfig {
        address: args.address.parse::<SocketAddr>()?,
        event_capacity: args.capacity,
        ..BackendConfig::default()
    };
    let max_query_events = config.max_query_events;
    let backend = Arc::new(Backend::start(config)?);
    let listener = TcpListener::bind(&*args.query_address)?;
    let running = Arc::new(AtomicBool::new(true));
    let worker_backend = Arc::clone(&backend);
    let worker_running = Arc::clone(&running);

    let worker = thread::spawn(move || {
        serve_queries(
            listener,
            worker_backend,
            worker_running,
            max_query_events,
            QUERY_WORKERS,
            QUERY_QUEUE_CAPACITY,
        )
    });

    println!("EventDock ingestion: {}", backend.address());
    println!("EventDock queries:   {}", args.query_address);

    tokio::signal::ctrl_c().await?;
    running.store(false, Ordering::Release);

    worker
        .join()
        .map_err(|_| io::Error::other("query listener panicked"))??;

    let backend = Arc::try_unwrap(backend)
        .map_err(|_| io::Error::other("backend still shared"))?;
    let snapshot = backend.shutdown()?;

    println!(
        "Stored {} events, rejected {}",
        snapshot.stored_total, snapshot.rejected_total
    );
    Ok(())
}
