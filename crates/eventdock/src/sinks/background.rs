use crate::{EventDockError, IncomingEvent, IncomingEventRef, LogSink};
use std::{
    io,
    sync::mpsc::{self, Receiver, RecvTimeoutError, SyncSender, TrySendError},
    thread::{self, JoinHandle},
    time::{Duration, Instant},
};

#[derive(Debug, Clone, Copy)]
pub struct BackgroundSinkConfig {
    pub queue_capacity: usize,
    pub batch_size: usize,
    pub flush_interval: Duration,
}

impl Default for BackgroundSinkConfig {
    fn default() -> Self {
        Self {
            queue_capacity: 4096,
            batch_size: 64,
            flush_interval: Duration::from_millis(10),
        }
    }
}

enum Command {
    Event(IncomingEvent),
    Flush(SyncSender<()>),
}

/// Moves sink operations to a dedicated worker. Successful submission means
/// queued, not delivered. A full queue rejects events with WouldBlock.
/// Call shutdown explicitly to drain the queue and observe worker errors.
#[derive(Debug)]
pub struct BackgroundSink<S> {
    sender: SyncSender<Command>,
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
        if config.queue_capacity == 0
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

        Ok(Self { sender, worker })
    }

    /// Drain accepted events, flush, join, and return the downstream sink.
    /// May block on downstream I/O. Reports the original worker error.
    pub fn shutdown(self) -> Result<S, EventDockError> {
        let Self { sender, worker } = self;
        drop(sender);

        worker
            .join()
            .map_err(|_| io::Error::other("background sink worker panicked"))?
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
        self.submit_owned(event.into())
    }

    fn submit_owned(
        &mut self,
        event: IncomingEvent,
    ) -> Result<(), EventDockError> {
        self.sender
            .try_send(Command::Event(event))
            .map_err(|error| match error {
                TrySendError::Full(_) => io::Error::new(
                    io::ErrorKind::WouldBlock,
                    "background sink queue is full",
                )
                .into(),
                TrySendError::Disconnected(_) => disconnected(),
            })
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
            Ok(Command::Event(event)) => {
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
