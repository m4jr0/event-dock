use crate::{
    BackendConfig, BackendSnapshot,
    query::{EventQuery, EventQueryResult},
    shared_snapshot::SharedSnapshot,
    storage::run_storage,
    transport::{CollectorLimits, Counters, collect},
};
use std::{
    io,
    net::{SocketAddr, TcpListener},
    sync::Arc,
    thread::{self, JoinHandle},
};
use tokio::sync::oneshot;

/// A pending query response. Dropping it cancels an unstarted query.
pub struct PendingQuery {
    response: flume::Receiver<EventQueryResult>,
}

impl PendingQuery {
    /// Poll for a completed result without blocking.
    pub fn try_recv(&self) -> io::Result<Option<EventQueryResult>> {
        match self.response.try_recv() {
            Ok(result) => Ok(Some(result)),
            Err(flume::TryRecvError::Empty) => Ok(None),
            Err(flume::TryRecvError::Disconnected) => Err(storage_stopped()),
        }
    }

    /// Wait until the query completes or the storage worker stops.
    pub fn recv(self) -> io::Result<EventQueryResult> {
        self.response
            .recv()
            .map_err(|_| storage_stopped())
    }
}

fn storage_stopped() -> io::Error {
    io::Error::new(io::ErrorKind::BrokenPipe, "storage worker stopped")
}

pub(crate) struct QueryRequest {
    pub query: EventQuery,
    pub reply: flume::Sender<EventQueryResult>,
}

const NETWORK_THREAD_NAME: &str = "eventdock-network";
const STORAGE_THREAD_NAME: &str = "eventdock-storage";

pub struct Backend {
    address: SocketAddr,
    stop: Option<oneshot::Sender<()>>,
    network: Option<JoinHandle<io::Result<()>>>,
    storage: Option<JoinHandle<()>>,
    snapshot: Arc<SharedSnapshot>,
    query_sender: flume::Sender<QueryRequest>,
    max_query_events: usize,
}

impl Backend {
    pub fn start(config: BackendConfig) -> io::Result<Self> {
        config.validate()?;

        let listener = bind_listener(&config)?;
        let address = listener.local_addr()?;
        let (sender, receiver) = flume::bounded(config.queue_capacity);
        let (query_sender, query_receiver) =
            flume::bounded(config.query_queue_capacity);
        let max_query_events = config.max_query_events;
        let (stop, stop_receiver) = oneshot::channel();
        let counters = Arc::new(Counters::default());
        let snapshot = Arc::new(SharedSnapshot::default());

        let limits = CollectorLimits {
            max_connections: config.max_connections,
            max_event_bytes: config.max_event_bytes,
        };

        let network = spawn_network_worker(
            listener,
            sender,
            stop_receiver,
            Arc::clone(&counters),
            limits,
        )?;

        let storage_result = spawn_storage_worker(
            config,
            receiver,
            Arc::clone(&snapshot),
            counters,
            query_receiver,
        );

        let storage = match storage_result {
            Ok(worker) => worker,

            Err(error) => {
                let _ = stop.send(());
                let _ = network.join();
                return Err(error);
            }
        };

        Ok(Self {
            address,
            query_sender,
            max_query_events,
            stop: Some(stop),
            network: Some(network),
            storage: Some(storage),
            snapshot,
        })
    }

    /// Submit a query without blocking the calling thread.
    /// A full request queue returns WouldBlock; callers can retry later.
    pub fn submit_query(
        &self,
        query: EventQuery,
    ) -> io::Result<PendingQuery> {
        if query.limit == 0 || query.limit > self.max_query_events {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "query limit must be between 1 and {}",
                    self.max_query_events
                ),
            ));
        }

        let (reply, response) = flume::bounded(1);
        let request = QueryRequest { query, reply };

        self.query_sender
            .try_send(request)
            .map_err(|error| match error {
                flume::TrySendError::Full(_) => io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "query queue is full",
                ),
                flume::TrySendError::Disconnected(_) => storage_stopped(),
            })?;

        Ok(PendingQuery { response })
    }

    /// Submit a query and wait for its result.
    pub fn query(
        &self,
        query: EventQuery,
    ) -> io::Result<EventQueryResult> {
        self.submit_query(query)?.recv()
    }

    pub fn address(&self) -> SocketAddr {
        self.address
    }

    /// Returns the latest periodically published snapshot without waiting for storage work.
    pub fn snapshot(&self) -> BackendSnapshot {
        self.snapshot.read()
    }

    pub fn is_running(&self) -> bool {
        self.network
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
            && self
                .storage
                .as_ref()
                .is_some_and(|worker| !worker.is_finished())
    }

    /// Stops intake, drains the accepted queue, and joins both threads.
    /// Events still waiting in client sockets or to enter the queue can be lost.
    pub fn shutdown(mut self) -> io::Result<BackendSnapshot> {
        self.request_stop();

        let network = self
            .network
            .take()
            .expect("network handle")
            .join();

        let storage = self
            .storage
            .take()
            .expect("storage handle")
            .join();

        storage.map_err(|_| io::Error::other("storage worker panicked"))?;
        network.map_err(|_| io::Error::other("network worker panicked"))??;

        Ok(self.snapshot())
    }

    fn request_stop(&mut self) {
        if let Some(stop) = self.stop.take() {
            let _ = stop.send(());
        }
    }
}

impl Drop for Backend {
    fn drop(&mut self) {
        self.request_stop();
    }
}

fn bind_listener(config: &BackendConfig) -> io::Result<TcpListener> {
    let listener = TcpListener::bind(config.address)?;
    listener.set_nonblocking(true)?;

    Ok(listener)
}

fn spawn_network_worker(
    listener: TcpListener,
    sender: flume::Sender<eventdock_core::IncomingEvent>,
    stop: oneshot::Receiver<()>,
    counters: Arc<Counters>,
    limits: CollectorLimits,
) -> io::Result<JoinHandle<io::Result<()>>> {
    let runtime = tokio::runtime::Builder::new_current_thread()
        .enable_all()
        .build()?;

    thread::Builder::new()
        .name(NETWORK_THREAD_NAME.into())
        .spawn(move || {
            runtime.block_on(async move {
                let listener = tokio::net::TcpListener::from_std(listener)?;
                collect(listener, sender, stop, counters, limits).await
            })
        })
}

fn spawn_storage_worker(
    config: BackendConfig,
    receiver: flume::Receiver<eventdock_core::IncomingEvent>,
    snapshot: Arc<SharedSnapshot>,
    counters: Arc<Counters>,
    queries: flume::Receiver<QueryRequest>,
) -> io::Result<JoinHandle<()>> {
    thread::Builder::new()
        .name(STORAGE_THREAD_NAME.into())
        .spawn(move || {
            run_storage(config, receiver, snapshot, counters, queries)
        })
}
