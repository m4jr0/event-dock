use crate::{EventDockError, IncomingEvent, IncomingEventRef, LogSink};
use std::{
    io,
    sync::{
        Arc,
        atomic::{AtomicUsize, Ordering},
        mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    },
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy)]
pub struct BackgroundSinkConfig {
    pub queue_capacity: usize,
    /// Limit estimated event bytes queued or being submitted downstream.
    pub byte_capacity: usize,
    pub batch_size: usize,
    pub flush_interval: Duration,
}

impl Default for BackgroundSinkConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 4096,
            byte_capacity: 32 * 1024 * 1024,
            batch_size: 64,
            flush_interval: Duration::from_millis(10),
        }
    }
}

enum Command {
    Event(IncomingEvent, BytePermit),
    Flush(SyncSender<()>),
}

/// Moves sink operations to a dedicated worker. Successful submission means
/// queued, not delivered. A full queue rejects events with WouldBlock.
/// Call shutdown explicitly to drain the queue and observe worker errors.
/// Dropping the wrapper closes the queue but detaches the worker. Neither
/// shutdown nor flush has a total deadline: downstream sinks may block.
#[derive(Debug)]
pub struct BackgroundSink<S> {
    sender: SyncSender<Command>,
    budget: ByteBudget,
    worker: JoinHandle<Result<S, EventDockError>>,
}

impl<S: LogSink + Send + 'static> BackgroundSink<S> {
    pub fn new(
        sink: S,
        queue_capacity: usize,
    ) -> Result<Self, EventDockError> {
        Self::with_config(
            sink,
            BackgroundSinkConfig {
                queue_capacity,
                ..BackgroundSinkConfig::default()
            },
        )
    }

    pub fn with_config(
        sink: S,
        config: BackgroundSinkConfig,
    ) -> Result<Self, EventDockError> {
        if config.byte_capacity == 0
            || config.queue_capacity == 0
            || config.batch_size == 0
            || config.flush_interval.is_zero()
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "background sink configuration values must be positive",
            )
            .into());
        }

        let (sender, receiver) = mpsc::sync_channel(config.queue_capacity);

        let worker = thread::Builder::new()
            .name("eventdock-sink".into())
            .spawn(move || run_worker(sink, receiver, config))?;

        Ok(Self {
            sender,
            budget: ByteBudget {
                capacity: config.byte_capacity,
                available: Arc::new(AtomicUsize::new(config.byte_capacity)),
            },
            worker,
        })
    }

    /// Drain accepted events, flush, join, and return the downstream sink.
    /// May block indefinitely on downstream I/O. Reports the original worker error.
    /// A finite downstream per-operation timeout does not bound the entire drain.
    pub fn shutdown(self) -> Result<S, EventDockError> {
        let Self { sender, worker, .. } = self;
        drop(sender);

        worker
            .join()
            .map_err(|_| io::Error::other("background sink worker panicked"))?
    }
}

// Account for event data and field entries, not allocator or tree-node overhead.
fn event_bytes(event: IncomingEventRef<'_>) -> usize {
    let mut bytes = std::mem::size_of::<IncomingEvent>()
        .saturating_add(event.source.len())
        .saturating_add(event.message.len());
    for (name, value) in event.fields.iter() {
        bytes = bytes
            .saturating_add(std::mem::size_of_val(name))
            .saturating_add(std::mem::size_of_val(value))
            .saturating_add(name.capacity());
        if let crate::FieldValue::String(value) = value {
            bytes = bytes.saturating_add(value.capacity());
        }
    }
    bytes
}

#[derive(Debug)]
struct ByteBudget {
    capacity: usize,
    available: Arc<AtomicUsize>,
}

struct BytePermit {
    available: Arc<AtomicUsize>,
    bytes: usize,
}

impl ByteBudget {
    fn reserve(
        &self,
        bytes: usize,
    ) -> Result<BytePermit, EventDockError> {
        if bytes > self.capacity {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "event exceeds background byte capacity",
            )
            .into());
        }
        self.available
            .try_update(Ordering::AcqRel, Ordering::Acquire, |available| {
                available.checked_sub(bytes)
            })
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "background byte capacity exhausted",
                )
            })?;
        Ok(BytePermit {
            available: Arc::clone(&self.available),
            bytes,
        })
    }
}

impl Drop for BytePermit {
    fn drop(&mut self) {
        self.available
            .fetch_add(self.bytes, Ordering::Release);
    }
}

fn disconnected() -> EventDockError {
    io::Error::new(
        io::ErrorKind::BrokenPipe,
        "background sink stopped; call shutdown for worker result",
    )
    .into()
}

impl<S: LogSink + Send + 'static> LogSink for BackgroundSink<S> {
    fn submit(
        &mut self,
        event: IncomingEventRef<'_>,
    ) -> Result<(), EventDockError> {
        let permit = self
            .budget
            .reserve(event_bytes(event))?;
        self.enqueue(event.into(), permit)
    }

    fn submit_owned(
        &mut self,
        event: IncomingEvent,
    ) -> Result<(), EventDockError> {
        let bytes = event_bytes(IncomingEventRef::from(&event))
            .saturating_add(event.source.capacity() - event.source.len())
            .saturating_add(event.message.capacity() - event.message.len());
        let permit = self.budget.reserve(bytes)?;
        self.enqueue(event, permit)
    }

    /// FIFO barrier; blocks until preceding events have been flushed.
    fn flush(&mut self) -> Result<(), EventDockError> {
        let (ack_sender, ack_receiver) = mpsc::sync_channel(1);
        self.sender
            .send(Command::Flush(ack_sender))
            .map_err(|_| disconnected())?;
        ack_receiver
            .recv()
            .map_err(|_| disconnected())
    }
}

impl<S> BackgroundSink<S> {
    fn enqueue(
        &self,
        event: IncomingEvent,
        permit: BytePermit,
    ) -> Result<(), EventDockError> {
        self.sender
            .try_send(Command::Event(event, permit))
            .map_err(|error| match error {
                TrySendError::Full(_) => io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "background sink queue is full",
                )
                .into(),
                TrySendError::Disconnected(_) => disconnected(),
            })
    }
}

fn should_flush(
    pending: usize,
    started: Instant,
    config: &BackgroundSinkConfig,
) -> bool {
    pending != 0
        && (pending >= config.batch_size
            || started.elapsed() >= config.flush_interval)
}

fn run_worker<S: LogSink>(
    mut sink: S,
    receiver: Receiver<Command>,
    config: BackgroundSinkConfig,
) -> Result<S, EventDockError> {
    let mut pending = 0;
    let mut started = Instant::now();

    loop {
        if should_flush(pending, started, &config) {
            sink.flush()?;
            pending = 0;
        }

        let command = if pending == 0 {
            receiver
                .recv()
                .map_err(|_| RecvTimeoutError::Disconnected)
        } else {
            let remaining_time = config
                .flush_interval
                .saturating_sub(started.elapsed());

            receiver.recv_timeout(remaining_time)
        };

        match command {
            Ok(Command::Event(event, _permit)) => {
                if pending == 0 {
                    started = Instant::now();
                }

                sink.submit_owned(event)?;
                pending += 1;
            }

            Ok(Command::Flush(ack_sender)) => {
                sink.flush()?;
                pending = 0;
                let _ = ack_sender.send(());
            }

            Err(RecvTimeoutError::Timeout) => {
                sink.flush()?;
                pending = 0;
            }

            Err(RecvTimeoutError::Disconnected) => {
                sink.flush()?;
                return Ok(sink);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{LogLevel, Logger};

    #[derive(Default)]
    struct RecordingSink {
        events: Vec<IncomingEvent>,
        flushes: usize,
    }

    impl LogSink for RecordingSink {
        fn submit(
            &mut self,
            event: IncomingEventRef<'_>,
        ) -> Result<(), EventDockError> {
            self.events.push(event.into());

            Ok(())
        }

        fn submit_owned(
            &mut self,
            event: IncomingEvent,
        ) -> Result<(), EventDockError> {
            self.events.push(event);

            Ok(())
        }

        fn flush(&mut self) -> Result<(), EventDockError> {
            self.flushes += 1;

            Ok(())
        }
    }

    #[test]
    fn byte_budget_releases_reservations() {
        let budget = ByteBudget {
            capacity: 10,
            available: Arc::new(AtomicUsize::new(10)),
        };
        let permit = budget.reserve(8).unwrap();
        assert!(
            matches!(budget.reserve(3), Err(EventDockError::Io(e)) if e.kind() == io::ErrorKind::WouldBlock)
        );
        assert!(
            matches!(budget.reserve(11), Err(EventDockError::Io(e)) if e.kind() == io::ErrorKind::InvalidInput)
        );
        drop(permit);
        assert!(budget.reserve(10).is_ok());
    }

    #[test]
    fn oversized_borrowed_event_is_rejected() {
        let config = BackgroundSinkConfig {
            byte_capacity: 1,
            ..BackgroundSinkConfig::default()
        };
        let mut sink =
            BackgroundSink::with_config(RecordingSink::default(), config)
                .unwrap();
        let event = IncomingEvent::new("foo", LogLevel::Info, "bar");
        assert!(
            matches!(sink.submit(IncomingEventRef::from(&event)), Err(EventDockError::Io(e)) if e.kind() == io::ErrorKind::InvalidInput)
        );
        assert!(
            sink.shutdown()
                .unwrap()
                .events
                .is_empty()
        );
    }

    #[test]
    fn preserves_order_and_owned_allocation() {
        let mut logger = Logger::new(
            "foo",
            BackgroundSink::new(RecordingSink::default(), 8).unwrap(),
        );

        let event = IncomingEvent::new("bar", LogLevel::Info, "owned");
        let address = event.message.as_ptr() as usize;

        logger.log_event(event).unwrap();
        logger.info("borrowed").unwrap();
        logger.flush().unwrap();

        let sink = logger.into_sink().shutdown().unwrap();

        assert_eq!(sink.events.len(), 2);
        assert_eq!(sink.events[0].message.as_ptr() as usize, address);
        assert_eq!(sink.events[1].message, "borrowed");
        assert!(sink.flushes >= 2);
    }
    struct GatedSink {
        entered: SyncSender<()>,
        release: Receiver<()>,
    }

    impl LogSink for GatedSink {
        fn submit(
            &mut self,
            _: IncomingEventRef<'_>,
        ) -> Result<(), EventDockError> {
            self.entered.send(()).unwrap();
            self.release.recv().unwrap();

            Ok(())
        }
    }

    #[test]
    fn full_queue_rejects_without_waiting() {
        let (entered, seen) = mpsc::sync_channel(2);
        let (release, gate) = mpsc::sync_channel(2);

        let mut logger = Logger::new(
            "foo",
            BackgroundSink::new(
                GatedSink {
                    entered,
                    release: gate,
                },
                1,
            )
            .unwrap(),
        );

        logger.info("first").unwrap();

        seen.recv_timeout(Duration::from_secs(5))
            .unwrap();

        logger.info("second").unwrap();

        let result = logger.info("third");
        release.send(()).unwrap();
        release.send(()).unwrap();

        assert!(
            matches!(result, Err(EventDockError::Io(e)) if e.kind() == io::ErrorKind::WouldBlock)
        );

        logger.into_sink().shutdown().unwrap();
    }

    struct NotifyingSink(SyncSender<()>);
    impl LogSink for NotifyingSink {
        fn submit(
            &mut self,
            _: IncomingEventRef<'_>,
        ) -> Result<(), EventDockError> {
            Ok(())
        }

        fn flush(&mut self) -> Result<(), EventDockError> {
            let _ = self.0.try_send(());

            Ok(())
        }
    }

    #[test]
    fn flushes_during_idle_period() {
        let (sender, receiver) = mpsc::sync_channel(1);

        let mut logger = Logger::new(
            "foo",
            BackgroundSink::new(NotifyingSink(sender), 8).unwrap(),
        );

        logger.info("one").unwrap();

        receiver
            .recv_timeout(Duration::from_secs(5))
            .unwrap();

        logger.into_sink().shutdown().unwrap();
    }

    struct FailingSink;
    impl LogSink for FailingSink {
        fn submit(
            &mut self,
            _: IncomingEventRef<'_>,
        ) -> Result<(), EventDockError> {
            Err(
                io::Error::new(io::ErrorKind::ConnectionReset, "test failure")
                    .into(),
            )
        }
    }

    #[test]
    fn shutdown_reports_worker_error() {
        let mut logger =
            Logger::new("foo", BackgroundSink::new(FailingSink, 8).unwrap());

        logger.info("one").unwrap();
        assert!(logger.flush().is_err());

        assert!(
            matches!(logger.into_sink().shutdown(), Err(EventDockError::Io(e)) if e.kind() == io::ErrorKind::ConnectionReset)
        );
    }
}
