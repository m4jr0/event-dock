use super::{CollectorLimits, Counters, tokio_tcp::collect};
use std::{
    io,
    net::TcpListener,
    sync::Arc,
    thread::{self, JoinHandle},
};
use tokio::{runtime::Handle, sync::oneshot, task::JoinHandle as TaskHandle};

pub(crate) struct NetworkRuntime {
    handle: Handle,
    stop: Option<oneshot::Sender<()>>,
    thread: Option<JoinHandle<()>>,
}

impl NetworkRuntime {
    pub(crate) fn new() -> io::Result<Self> {
        let runtime = tokio::runtime::Builder::new_current_thread()
            .enable_all()
            .build()?;
        let handle = runtime.handle().clone();
        let (stop, stopped) = oneshot::channel();
        let thread = thread::Builder::new()
            .name("eventdock-network".into())
            .spawn(move || {
                runtime.block_on(async {
                    let _ = stopped.await;
                })
            })?;
        Ok(Self {
            handle,
            stop: Some(stop),
            thread: Some(thread),
        })
    }

    pub(crate) fn spawn_collector(
        &self,
        listener: TcpListener,
        sender: flume::Sender<crate::ingestion::AdmittedEvent>,
        counters: Arc<Counters>,
        limits: CollectorLimits,
    ) -> CollectorTask {
        let (stop, stopped) = oneshot::channel();
        let (reply, result) = flume::bounded(1);
        let task = self.handle.spawn(async move {
            let outcome = async {
                let listener = tokio::net::TcpListener::from_std(listener)?;
                collect(listener, sender, stopped, counters, limits).await
            }
            .await;
            let _ = reply.send(outcome);
        });
        CollectorTask {
            stop: Some(stop),
            task,
            result,
        }
    }
}

impl Drop for NetworkRuntime {
    fn drop(&mut self) {
        self.stop.take();
        if let Some(thread) = self.thread.take() {
            let _ = thread.join();
        }
    }
}

pub(crate) struct CollectorTask {
    stop: Option<oneshot::Sender<()>>,
    task: TaskHandle<()>,
    result: flume::Receiver<io::Result<()>>,
}

impl CollectorTask {
    pub(crate) fn is_finished(&self) -> bool {
        self.task.is_finished()
    }

    pub(crate) fn stop(mut self) -> io::Result<()> {
        self.stop.take();
        self.result.recv().map_err(|_| {
            io::Error::other("collector task stopped unexpectedly")
        })?
    }
}

impl Drop for CollectorTask {
    fn drop(&mut self) {
        self.stop.take();
    }
}
