use crate::{EventDockError, IncomingEventRef, LogSink};
use std::{
    io::{self, BufWriter, Write},
    net::{TcpStream, ToSocketAddrs},
    time::Duration,
};

#[derive(Debug, Clone, Copy)]
pub struct TcpSinkConfig {
    buffer_capacity: usize,
    nodelay: bool,
    write_timeout: Option<Duration>,
    flush_on_submit: bool,
}

impl Default for TcpSinkConfig {
    fn default() -> Self {
        Self {
            buffer_capacity: 8 * 1024,
            nodelay: true,
            write_timeout: Some(Duration::from_secs(5)),
            flush_on_submit: true,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TcpSinkState {
    Connected,
    Failed,
}

impl TcpSinkConfig {
    fn validate(&self) -> Result<(), EventDockError> {
        if self.buffer_capacity == 0 {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "tcp buffer capacity must be greater than zero",
            )
            .into());
        }

        if self.write_timeout == Some(Duration::ZERO) {
            return Err(io::Error::new(
                io::ErrorKind::InvalidInput,
                "tcp write timeout must be greater than zero",
            )
            .into());
        }

        Ok(())
    }

    #[must_use]
    pub fn with_buffer_capacity(
        mut self,
        capacity: usize,
    ) -> Self {
        self.buffer_capacity = capacity;
        self
    }

    #[must_use]
    pub fn with_nodelay(
        mut self,
        enabled: bool,
    ) -> Self {
        self.nodelay = enabled;
        self
    }

    /// Set the per-write socket timeout. None permits indefinite blocking.
    #[must_use]
    pub fn with_write_timeout(
        mut self,
        timeout: Option<Duration>,
    ) -> Self {
        self.write_timeout = timeout;
        self
    }

    /// When disabled, callers must explicitly flush buffered output.
    #[must_use]
    pub fn with_flush_on_submit(
        mut self,
        enabled: bool,
    ) -> Self {
        self.flush_on_submit = enabled;
        self
    }

    pub fn flush_on_submit(&self) -> bool {
        self.flush_on_submit
    }

    pub fn buffer_capacity(&self) -> usize {
        self.buffer_capacity
    }

    pub fn nodelay(&self) -> bool {
        self.nodelay
    }

    pub fn write_timeout(&self) -> Option<Duration> {
        self.write_timeout
    }
}

/// Sends newline-delimited JSON events over a persistent TCP connection.
#[derive(Debug)]
pub struct TcpSink {
    writer: Option<BufWriter<TcpStream>>,
    staging: Vec<u8>,
    state: TcpSinkState,
    flush_on_submit: bool,
}

impl TcpSink {
    pub fn connect(
        address: impl ToSocketAddrs
    ) -> Result<Self, EventDockError> {
        Self::connect_with_config(address, TcpSinkConfig::default())
    }

    pub fn connect_with_config(
        address: impl ToSocketAddrs,
        config: TcpSinkConfig,
    ) -> Result<Self, EventDockError> {
        config.validate()?;

        let stream = TcpStream::connect(address)?;
        stream.set_nodelay(config.nodelay)?;
        stream.set_write_timeout(config.write_timeout)?;

        Ok(Self {
            writer: Some(BufWriter::with_capacity(
                config.buffer_capacity,
                stream,
            )),
            staging: Vec::new(),
            state: TcpSinkState::Connected,
            flush_on_submit: config.flush_on_submit,
        })
    }

    pub fn state(&self) -> TcpSinkState {
        self.state
    }

    fn serialize(
        &mut self,
        event: IncomingEventRef<'_>,
    ) -> Result<(), EventDockError> {
        self.staging.clear();

        serde_json::to_writer(&mut self.staging, &event)?;
        self.staging.push(b'\n');

        Ok(())
    }

    pub fn flush(&mut self) -> Result<(), EventDockError> {
        self.ensure_connected()?;

        if let Err(error) = self
            .writer
            .as_mut()
            .expect("connected writer")
            .flush()
        {
            self.fail();
            return Err(error.into());
        }

        Ok(())
    }

    fn fail(&mut self) {
        self.state = TcpSinkState::Failed;
        if let Some(writer) = self.writer.take() {
            // Discard buffered output without retrying failed writes on drop.
            let _ = writer.into_parts();
        }
    }

    fn ensure_connected(&self) -> Result<(), EventDockError> {
        if self.state == TcpSinkState::Failed {
            return Err(io::Error::new(
                io::ErrorKind::BrokenPipe,
                "tcp sink is no longer usable",
            )
            .into());
        }

        Ok(())
    }
}

impl LogSink for TcpSink {
    fn submit(
        &mut self,
        event: IncomingEventRef<'_>,
    ) -> Result<(), EventDockError> {
        self.ensure_connected()?;
        self.serialize(event)?;

        if let Err(error) = self
            .writer
            .as_mut()
            .expect("connected writer")
            .write_all(&self.staging)
        {
            self.fail();
            return Err(error.into());
        }

        if self.flush_on_submit {
            self.flush()
        } else {
            Ok(())
        }
    }

    fn flush(&mut self) -> Result<(), EventDockError> {
        TcpSink::flush(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{IncomingEvent, LogLevel};
    use std::{
        io::{BufRead, BufReader},
        net::SocketAddr,
        net::TcpListener,
        thread,
        time::Instant,
    };

    const TEST_TIMEOUT: Duration = Duration::from_secs(5);

    fn make_event(message: &str) -> IncomingEvent {
        IncomingEvent::new("foo", LogLevel::Info, message)
    }

    fn start_receiver(
        expected_lines: usize
    ) -> (SocketAddr, thread::JoinHandle<io::Result<Vec<String>>>) {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        listener.set_nonblocking(true).unwrap();

        let address = listener.local_addr().unwrap();

        let receiver = thread::spawn(move || {
            let deadline = Instant::now() + TEST_TIMEOUT;

            let stream = loop {
                match listener.accept() {
                    Ok((stream, _)) => break stream,

                    Err(error) if error.kind() == io::ErrorKind::WouldBlock => {
                        if Instant::now() >= deadline {
                            return Err(io::Error::new(
                                io::ErrorKind::TimedOut,
                                "timed out waiting for TCP connection",
                            ));
                        }

                        thread::sleep(Duration::from_millis(5));
                    }

                    Err(error) => return Err(error),
                }
            };

            stream.set_read_timeout(Some(TEST_TIMEOUT))?;

            BufReader::new(stream)
                .lines()
                .take(expected_lines)
                .collect()
        });

        (address, receiver)
    }

    #[test]
    fn serialization_produces_newline_delimited_json() {
        let (address, receiver) = start_receiver(1);

        let mut sink = TcpSink::connect(address).unwrap();
        let event = make_event("foo");
        sink.submit(IncomingEventRef::from(&event))
            .unwrap();

        let lines = receiver.join().unwrap().unwrap();
        assert_eq!(lines.len(), 1);

        let event: IncomingEvent = serde_json::from_str(&lines[0]).unwrap();
        assert_eq!(event.source, "foo");
        assert_eq!(event.message, "foo");
        assert_eq!(event.level, LogLevel::Info);
    }

    #[test]
    fn multiple_events_have_separate_json_lines() {
        let (address, receiver) = start_receiver(3);
        let mut sink = TcpSink::connect(address).unwrap();

        for message in ["foo", "bar", "baz"] {
            let event = make_event(message);
            sink.submit(IncomingEventRef::from(&event))
                .unwrap();
        }

        let lines = receiver.join().unwrap().unwrap();
        assert_eq!(lines.len(), 3);

        for (line, expected) in lines.iter().zip(["foo", "bar", "baz"]) {
            let event: IncomingEvent = serde_json::from_str(line).unwrap();
            assert_eq!(event.message, expected);
        }
    }

    #[test]
    fn serialization_reuses_staging_capacity() {
        let (address, receiver) = start_receiver(2);

        let mut sink = TcpSink::connect(address).unwrap();

        let long_event = make_event(&"x".repeat(1024));
        sink.submit(IncomingEventRef::from(&long_event))
            .unwrap();

        let capacity = sink.staging.capacity();
        assert!(capacity > 0);

        let short_event = make_event("bar");
        sink.submit(IncomingEventRef::from(&short_event))
            .unwrap();

        assert_eq!(sink.staging.capacity(), capacity);
        assert_eq!(receiver.join().unwrap().unwrap().len(), 2);
    }

    #[test]
    fn failed_sink_rejects_subsequent_events() {
        let (address, receiver) = start_receiver(0);

        let mut sink = TcpSink::connect(address).unwrap();
        sink.state = TcpSinkState::Failed;

        let event = make_event("foo");
        let result = sink.submit(IncomingEventRef::from(&event));

        assert!(matches!(
            result,
            Err(EventDockError::Io(error))
                if error.kind() == io::ErrorKind::BrokenPipe
        ));

        assert_eq!(sink.state(), TcpSinkState::Failed);
        receiver.join().unwrap().unwrap();
    }

    #[test]
    fn rejects_zero_buffer_capacity() {
        let config = TcpSinkConfig::default().with_buffer_capacity(0);

        let result = TcpSink::connect_with_config("127.0.0.1:1", config);

        assert!(matches!(
            result,
            Err(EventDockError::Io(error))
                if error.kind() == io::ErrorKind::InvalidInput
        ));
    }

    #[test]
    fn rejects_zero_write_timeout() {
        let config =
            TcpSinkConfig::default().with_write_timeout(Some(Duration::ZERO));

        let result = TcpSink::connect_with_config("127.0.0.1:1", config);

        assert!(matches!(
            result,
            Err(EventDockError::Io(error))
                if error.kind() == io::ErrorKind::InvalidInput
        ));
    }

    #[test]
    fn config_builder_preserves_values() {
        let config = TcpSinkConfig::default()
            .with_buffer_capacity(16 * 1024)
            .with_nodelay(false)
            .with_write_timeout(Some(Duration::from_secs(2)));

        assert_eq!(config.buffer_capacity(), 16 * 1024);
        assert!(!config.nodelay());
        assert_eq!(config.write_timeout(), Some(Duration::from_secs(2)));
    }

    #[test]
    fn starts_connected() {
        let (address, receiver) = start_receiver(0);

        let sink = TcpSink::connect(address).unwrap();
        assert_eq!(sink.state(), TcpSinkState::Connected);

        receiver.join().unwrap().unwrap();
    }

    #[test]
    fn buffered_mode_waits_for_explicit_flush() {
        let (address, receiver) = start_receiver(2);
        let config = TcpSinkConfig::default().with_flush_on_submit(false);
        let mut sink = TcpSink::connect_with_config(address, config).unwrap();

        for message in ["first", "second"] {
            let event = make_event(message);
            sink.submit(IncomingEventRef::from(&event))
                .unwrap();
        }

        assert!(
            !sink
                .writer
                .as_ref()
                .unwrap()
                .buffer()
                .is_empty()
        );
        LogSink::flush(&mut sink).unwrap();
        assert!(
            sink.writer
                .as_ref()
                .unwrap()
                .buffer()
                .is_empty()
        );

        let lines = receiver.join().unwrap().unwrap();
        let messages: Vec<String> = lines
            .iter()
            .map(|line| {
                serde_json::from_str::<IncomingEvent>(line)
                    .unwrap()
                    .message
            })
            .collect();
        assert_eq!(messages, ["first", "second"]);
    }

    #[test]
    fn default_write_timeout_is_finite() {
        assert_eq!(
            TcpSinkConfig::default().write_timeout(),
            Some(Duration::from_secs(5))
        );
        assert_eq!(
            TcpSinkConfig::default()
                .with_write_timeout(None)
                .write_timeout(),
            None
        );
    }

    #[test]
    fn failed_sink_discards_buffered_output() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let config = TcpSinkConfig::default().with_flush_on_submit(false);
        let mut sink = TcpSink::connect_with_config(
            listener.local_addr().unwrap(),
            config,
        )
        .unwrap();
        let (stream, _) = listener.accept().unwrap();
        stream
            .set_read_timeout(Some(TEST_TIMEOUT))
            .unwrap();
        let event = make_event("discard me");
        sink.submit(IncomingEventRef::from(&event))
            .unwrap();
        assert!(
            !sink
                .writer
                .as_ref()
                .unwrap()
                .buffer()
                .is_empty()
        );
        sink.fail();
        assert_eq!(sink.state(), TcpSinkState::Failed);
        assert!(sink.writer.is_none());
        let mut line = String::new();
        assert_eq!(
            BufReader::new(stream)
                .read_line(&mut line)
                .unwrap(),
            0
        );
    }

    #[test]
    fn default_still_flushes_on_submit() {
        assert!(TcpSinkConfig::default().flush_on_submit());
    }

    #[test]
    fn serialization_replaces_previous_staging_contents() {
        let (address, receiver) = start_receiver(0);

        let mut sink = TcpSink::connect(address).unwrap();
        let first = make_event("foo");
        let second = make_event("bar");
        sink.serialize(IncomingEventRef::from(&first))
            .unwrap();
        sink.serialize(IncomingEventRef::from(&second))
            .unwrap();

        let serialized = std::str::from_utf8(&sink.staging).unwrap();

        assert!(serialized.ends_with('\n'));
        assert_eq!(serialized.matches('\n').count(), 1);

        let event: IncomingEvent = serde_json::from_str(serialized).unwrap();

        assert_eq!(event.source, "foo");
        assert_eq!(event.message, "bar");

        receiver.join().unwrap().unwrap();
    }
}
