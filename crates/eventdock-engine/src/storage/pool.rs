use super::worker::{StorageWorker, StoreTask, create_store};
use crate::{
    BackendConfig, backend::QueryRequest, shared_snapshot::SharedSnapshot,
    transport::Counters,
};
use eventdock_core::IncomingEvent;
use std::{
    io,
    sync::Arc,
    thread::{self, JoinHandle},
};

struct WorkerSlot {
    registrations: flume::Sender<StoreTask>,
    thread: JoinHandle<()>,
    assigned: usize,
}

pub(crate) struct StoragePool {
    workers: Vec<WorkerSlot>,
}

impl StoragePool {
    pub(crate) fn new(count: usize) -> io::Result<Self> {
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
            let (registrations, receiver) = flume::bounded(1);
            let thread = thread::Builder::new()
                .name(format!("eventdock-storage-{index}"))
                .spawn(move || StorageWorker::new(receiver).run())?;
            pool.workers.push(WorkerSlot {
                registrations,
                thread,
                assigned: 0,
            });
        }
        Ok(pool)
    }

    pub(crate) fn register(
        &mut self,
        config: BackendConfig,
        receiver: flume::Receiver<IncomingEvent>,
        snapshot: Arc<SharedSnapshot>,
        counters: Arc<Counters>,
        queries: flume::Receiver<QueryRequest>,
    ) -> io::Result<()> {
        let worker = self
            .workers
            .iter_mut()
            .filter(|worker| !worker.thread.is_finished())
            .min_by_key(|worker| worker.assigned)
            .ok_or_else(|| io::Error::other("no storage worker available"))?;
        worker
            .registrations
            .send(create_store(config, receiver, snapshot, counters, queries))
            .map_err(|_| io::Error::other("storage worker stopped"))?;
        worker.assigned += 1;
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
