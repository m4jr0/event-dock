use super::counters::NetworkCounters;
use super::error::ClientError;
use eventdock::IncomingEvent;
use flume::Sender;
use futures_util::StreamExt;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_util::codec::{FramedRead, LinesCodec};

pub(crate) async fn read_client(
    stream: TcpStream,
    sender: Sender<IncomingEvent>,
    counters: Arc<NetworkCounters>,
    max_event_bytes: usize,
) -> Result<(), ClientError> {
    let mut lines = FramedRead::new(
        stream,
        LinesCodec::new_with_max_length(max_event_bytes),
    );

    while let Some(line) = lines.next().await {
        handle_line(line.map_err(ClientError::from)?, &sender, &counters)
            .await?;
    }

    Ok(())
}

async fn handle_line(
    line: String,
    sender: &Sender<IncomingEvent>,
    counters: &NetworkCounters,
) -> Result<(), ClientError> {
    if line.trim().is_empty() {
        return Ok(());
    }

    let event = match serde_json::from_str(&line) {
        Ok(event) => event,

        Err(_) => {
            counters.register_malformed_event();

            return Ok(());
        }
    };

    sender
        .send_async(event)
        .await
        .map_err(|_| ClientError::QueueClosed)
}
