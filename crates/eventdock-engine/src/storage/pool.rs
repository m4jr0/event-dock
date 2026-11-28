use super::worker::{StorageWorker, StoreTask, create_store};
use crate::{
    BackendConfig, backend::QueryRequest, shared_snapshot::SharedSnapshot,
    transport::Counters,
};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
    },
    thread::{self, JoinHandle},
};

struct WorkerSlot {
    registrations: flume::Sender<Box<StoreTask>>,
    thread: JoinHandle<()>,
    assigned: Arc<AtomicUsize>,
}

pub(crate) struct StoragePool {
    workers: Vec<WorkerSlot>,
}

impl StoragePool {
    pub(crate) fn new(count: usize) -> io::Result<Self> {
        Self::with_worker_factory(count, Self::spawn_worker)
    }

    fn with_worker_factory(
        count: usize,
        mut spawn: impl FnMut(usize) -> io::Result<WorkerSlot>,
    ) -> io::Result<Self> {
        if count == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "empty storage pool",
            ));
        }
        let mut pool = Self {
            workers: Vec::new(),
        };
        for index in 0..count {
            match spawn(index) {
                Ok(worker) => pool.workers.push(worker),
                Err(error) => {
                    // Join previously started workers before returning the startup error.
                    let _ = pool.shutdown();
                    return Err(error);
                }
            }
        }
        Ok(pool)
    }

    fn spawn_worker(index: usize) -> io::Result<WorkerSlot> {
        let (registrations, receiver) = flume::bounded(1);
        let thread = thread::Builder::new()
            .name(format!("eventdock-storage-{index}"))
            .spawn(move || StorageWorker::new(receiver).run())?;
        Ok(WorkerSlot {
            registrations,
            thread,
            assigned: Arc::new(AtomicUsize::new(0)),
        })
    }

    pub(crate) fn register(
        &mut self,
        config: BackendConfig,
        receiver: flume::Receiver<crate::ingestion::AdmittedEvent>,
        snapshot: Arc<SharedSnapshot>,
        counters: Arc<Counters>,
        queries: flume::Receiver<QueryRequest>,
    ) -> io::Result<()> {
        let worker = self
            .workers
            .iter_mut()
            .filter(|worker| !worker.thread.is_finished())
            .min_by_key(|worker| worker.assigned.load(Ordering::Acquire))
            .ok_or_else(|| io::Error::other("no storage worker available"))?;
        let mut store =
            create_store(config, receiver, snapshot, counters, queries);
        store.assign(Arc::clone(&worker.assigned));
        worker
            .registrations
            .send(Box::new(store))
            .map_err(|_| io::Error::other("storage worker stopped"))?;
        Ok(())
    }

    pub(crate) fn is_finished(&self) -> bool {
        self.workers
            .iter()
            .any(|worker| worker.thread.is_finished())
    }

    pub(crate) fn shutdown(mut self) -> io::Result<()> {
        let threads: Vec<_> = self
            .workers
            .drain(..)
            .map(|worker| {
                drop(worker.registrations);
                worker.thread
            })
            .collect();
        let mut failed = false;
        for thread in threads {
            failed |= thread.join().is_err();
        }
        if failed {
            Err(io::Error::other("storage worker panicked"))
        } else {
            Ok(())
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{EventQuery, EventQueryResult};
    use eventdock_core::LogLevel;
    use std::time::{Duration, Instant};

    const TIMEOUT: Duration = Duration::from_secs(5);

    struct TestStore {
        events: flume::Sender<crate::ingestion::AdmittedEvent>,
        queries: flume::Sender<QueryRequest>,
        snapshot: Arc<SharedSnapshot>,
    }

    impl TestStore {
        fn register(pool: &mut StoragePool) -> Self {
            let (events, receiver) = flume::bounded(64);
            let (queries, requests) = flume::bounded(8);
            let snapshot = Arc::new(SharedSnapshot::default());
            pool.register(
                BackendConfig {
                    query_scan_budget: 1,
                    snapshot_interval: Duration::from_millis(5),
                    ..BackendConfig::default()
                },
                receiver,
                Arc::clone(&snapshot),
                Arc::new(Counters::default()),
                requests,
            )
            .unwrap();
            Self {
                events,
                queries,
                snapshot,
            }
        }

        fn send(
            &self,
            message: &str,
        ) {
            self.events
                .send_timeout(
                    crate::ingestion::AdmittedEvent::for_test(
                        eventdock_core::IncomingEvent::new(
                            "test",
                            LogLevel::Info,
                            message,
                        ),
                    ),
                    TIMEOUT,
                )
                .unwrap();
        }

        fn wait_for_events(
            &self,
            count: u64,
        ) {
            let deadline = Instant::now() + TIMEOUT;
            while self.snapshot.read().stored_total < count {
                assert!(Instant::now() < deadline, "ingestion stalled");
                thread::sleep(Duration::from_millis(1));
            }
        }

        fn submit(&self) -> flume::Receiver<EventQueryResult> {
            let (reply, response) = flume::bounded(1);
            self.queries
                .send_timeout(
                    QueryRequest {
                        query: EventQuery {
                            before_id: None,
                            limit: 64,
                            level: None,
                        },
                        reply: crate::backend::Reply::Legacy(reply),
                    },
                    TIMEOUT,
                )
                .unwrap();
            response
        }
    }

    fn shutdown(pool: StoragePool) {
        let (done, result) = flume::bounded(1);
        let thread = thread::spawn(move || {
            let _ = done.send(pool.shutdown());
        });
        result
            .recv_timeout(TIMEOUT)
            .expect("shutdown stalled")
            .unwrap();
        thread.join().unwrap();
    }

    fn exercise_isolation(count: usize) {
        let mut pool = StoragePool::new(count).unwrap();
        let first = TestStore::register(&mut pool);
        let second = TestStore::register(&mut pool);
        for _ in 0..32 {
            first.send("first");
        }
        second.send("second");
        first.wait_for_events(32);
        second.wait_for_events(1);
        let abandoned = first.submit();
        drop(abandoned);
        let a = first.submit();
        let b = second.submit();
        let b = b.recv_timeout(TIMEOUT).unwrap();
        let a = a.recv_timeout(TIMEOUT).unwrap();
        assert_eq!(a.events.len(), 32);
        assert!(
            a.events
                .iter()
                .all(|event| event.message == "first")
        );
        assert_eq!(a.events[31].id, 0);
        assert_eq!(b.events.len(), 1);
        assert_eq!(b.events[0].message, "second");
        assert_eq!(b.events[0].id, 0);
        drop(first);
        second.send("still running");
        second.wait_for_events(2);
        drop(second);
        shutdown(pool);
    }

    #[test]
    fn stores_on_same_worker_are_isolated() {
        exercise_isolation(1);
    }

    #[test]
    fn stores_on_different_workers_are_isolated() {
        exercise_isolation(2);
    }

    #[test]
    fn registration_balances_assignments() {
        let mut pool = StoragePool::new(2).unwrap();
        let stores: Vec<_> = (0..5)
            .map(|_| TestStore::register(&mut pool))
            .collect();
        assert_eq!(
            pool.workers
                .iter()
                .map(|worker| worker.assigned.load(Ordering::Acquire))
                .collect::<Vec<_>>(),
            [3, 2]
        );
        drop(stores);
        shutdown(pool);
    }

    #[test]
    fn registration_reuses_worker_after_store_closes() {
        let mut pool = StoragePool::new(2).unwrap();
        let first = TestStore::register(&mut pool);
        let second = TestStore::register(&mut pool);
        let third = TestStore::register(&mut pool);
        drop(first);
        drop(third);
        let deadline = Instant::now() + TIMEOUT;
        while pool.workers[0]
            .assigned
            .load(Ordering::Acquire)
            != 0
        {
            assert!(Instant::now() < deadline, "store destruction stalled");
            thread::sleep(Duration::from_millis(1));
        }
        let replacement = TestStore::register(&mut pool);
        assert_eq!(
            pool.workers[0]
                .assigned
                .load(Ordering::Acquire),
            1
        );
        assert_eq!(
            pool.workers[1]
                .assigned
                .load(Ordering::Acquire),
            1
        );
        drop(replacement);
        drop(second);
        shutdown(pool);
    }

    #[test]
    fn startup_failure_joins_started_workers() {
        let exited = Arc::new(AtomicUsize::new(0));
        let result = StoragePool::with_worker_factory(3, |index| {
            if index == 2 {
                return Err(io::Error::other("injected startup failure"));
            }
            let (registrations, receiver) = flume::bounded(1);
            let exited = Arc::clone(&exited);
            let thread = thread::Builder::new().spawn(move || {
                StorageWorker::new(receiver).run();
                exited.fetch_add(1, Ordering::Release);
            })?;
            Ok(WorkerSlot {
                registrations,
                thread,
                assigned: Arc::new(AtomicUsize::new(0)),
            })
        });
        let error = result.err().expect("startup must fail");
        assert_eq!(error.to_string(), "injected startup failure");
        assert_eq!(exited.load(Ordering::Acquire), 2);
    }

    #[test]
    fn empty_pool_shuts_down() {
        shutdown(StoragePool::new(3).unwrap());
    }

    #[test]
    fn shutdown_drains_admitted_events_and_queries() {
        let mut pool = StoragePool::new(1).unwrap();
        let store = TestStore::register(&mut pool);
        for _ in 0..32 {
            store.send("retained");
        }
        store.wait_for_events(32);
        let response = store.submit();
        let snapshot = Arc::clone(&store.snapshot);
        store.send("last");
        drop(store);
        shutdown(pool);
        assert!(
            response
                .recv_timeout(TIMEOUT)
                .unwrap()
                .events
                .len()
                >= 32
        );
        assert_eq!(snapshot.read().stored_total, 33);
    }
}
