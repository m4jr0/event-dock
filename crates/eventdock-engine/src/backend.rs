use crate::{
    BackendConfig, BackendSnapshot,
    shared_snapshot::SharedSnapshot,
    storage::StoragePool,
    transport::{CollectorLimits, CollectorTask, Counters, NetworkRuntime},
};
use eventdock_protocol::{EventQuery, EventQueryResult};
use std::{
    io,
    net::{SocketAddr, TcpListener},
    sync::Arc,
};

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
    pub reply: Reply,
}

pub(crate) enum Reply {
    Legacy(flume::Sender<EventQueryResult>),
    Client(eventdock_protocol::QueryReply),
}

impl Reply {
    pub(crate) fn is_disconnected(&self) -> bool {
        match self {
            Self::Legacy(reply) => reply.is_disconnected(),
            Self::Client(reply) => reply.is_cancelled(),
        }
    }

    pub(crate) fn complete(
        self,
        result: EventQueryResult,
    ) {
        match self {
            Self::Legacy(reply) => {
                let _ = reply.try_send(result);
            }
            Self::Client(reply) => {
                reply.complete(Ok(result.into()));
            }
        }
    }
}

pub struct Backend {
    address: SocketAddr,
    network: Option<CollectorTask>,
    _network_runtime: NetworkRuntime,
    storage: Option<StoragePool>,
    snapshot: Arc<SharedSnapshot>,
    query_sender: Option<flume::Sender<QueryRequest>>,
    max_query_events: usize,
    ingestion_budget: crate::ByteBudget,
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
        let counters = Arc::new(Counters::default());
        let snapshot = Arc::new(SharedSnapshot::default());

        let ingestion_budget =
            crate::ByteBudget::new(config.ingestion_byte_capacity)
                .expect("validated ingestion budget");
        let limits = CollectorLimits {
            max_connections: config.max_connections,
            max_event_bytes: config.max_event_bytes,
            budget: ingestion_budget.clone(),
        };

        let network_runtime = NetworkRuntime::new()?;
        let network = network_runtime.spawn_collector(
            listener,
            sender,
            Arc::clone(&counters),
            limits,
        );

        let storage_result = StoragePool::new(config.storage_worker_count)
            .and_then(|mut pool| {
                pool.register(
                    config,
                    receiver,
                    Arc::clone(&snapshot),
                    counters,
                    query_receiver,
                )?;
                Ok(pool)
            });

        let storage = match storage_result {
            Ok(worker) => worker,

            Err(error) => {
                let _ = network.stop();
                return Err(error);
            }
        };

        Ok(Self {
            address,
            query_sender: Some(query_sender),
            max_query_events,
            ingestion_budget,
            network: Some(network),
            _network_runtime: network_runtime,
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
        let (reply, response) = flume::bounded(1);
        self.enqueue_query(query, Reply::Legacy(reply))?;
        Ok(PendingQuery { response })
    }

    /// Submit a typed client request without serialization or blocking.
    pub fn submit_client_query(
        &self,
        request: eventdock_protocol::QueryRequest,
    ) -> io::Result<eventdock_protocol::PendingQuery> {
        let (reply, pending) = eventdock_protocol::query_reply_channel();
        self.enqueue_query(
            EventQuery {
                limit: request.limit,
                before_id: request.before_id,
                level: request.level,
            },
            Reply::Client(reply),
        )?;
        Ok(pending)
    }

    fn enqueue_query(
        &self,
        query: EventQuery,
        reply: Reply,
    ) -> io::Result<()> {
        if query.limit == 0 || query.limit > self.max_query_events {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                format!(
                    "query limit must be between 1 and {}",
                    self.max_query_events
                ),
            ));
        }

        let request = QueryRequest { query, reply };

        self.query_sender
            .as_ref()
            .ok_or_else(storage_stopped)?
            .try_send(request)
            .map_err(|error| match error {
                flume::TrySendError::Full(_) => io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "query queue is full",
                ),
                flume::TrySendError::Disconnected(_) => storage_stopped(),
            })?;

        Ok(())
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
        let mut snapshot = self.snapshot.read();
        snapshot.ingestion_budget = self.ingestion_budget.usage();
        snapshot
    }

    pub fn is_running(&self) -> bool {
        self.storage
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
    }

    /// Stop the ingestion source without destroying retained events.
    /// Events already admitted to the queue continue to be processed.
    pub fn stop_ingestion(&mut self) -> io::Result<()> {
        if let Some(network) = self.network.take() {
            network.stop()?;
        }
        Ok(())
    }

    /// Report whether the ingestion worker is still running.
    pub fn is_ingesting(&self) -> bool {
        self.network
            .as_ref()
            .is_some_and(|worker| !worker.is_finished())
    }

    /// Stops intake, drains the accepted queue, and joins both threads.
    /// Events still waiting in client sockets or to enter the queue can be lost.
    pub fn shutdown(mut self) -> io::Result<BackendSnapshot> {
        let network = self.stop_ingestion();
        // Close the query channel to terminate retained storage after intake drains.
        self.query_sender.take();

        let storage = self
            .storage
            .take()
            .expect("storage handle")
            .shutdown();

        storage?;
        network?;

        Ok(self.snapshot())
    }
}

fn bind_listener(config: &BackendConfig) -> io::Result<TcpListener> {
    let listener = TcpListener::bind(config.address)?;
    listener.set_nonblocking(true)?;

    Ok(listener)
}
