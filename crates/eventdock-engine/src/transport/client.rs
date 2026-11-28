use super::counters::NetworkCounters;
use super::error::ClientError;
use flume::Sender;
use futures_util::StreamExt;
use std::sync::Arc;
use tokio::net::TcpStream;
use tokio_util::codec::{FramedRead, LinesCodec};

pub(crate) async fn read_client(
    stream: TcpStream,
    sender: Sender<crate::ingestion::AdmittedEvent>,
    counters: Arc<NetworkCounters>,
    max_event_bytes: usize,
    budget: crate::ByteBudget,
) -> Result<(), ClientError> {
    let mut lines = FramedRead::new(
        stream,
        LinesCodec::new_with_max_length(max_event_bytes),
    );

    while let Some(line) = lines.next().await {
        handle_line(
            line.map_err(ClientError::from)?,
            &sender,
            &counters,
            &budget,
        )
        .await?;
    }

    Ok(())
}

async fn handle_line(
    line: String,
    sender: &Sender<crate::ingestion::AdmittedEvent>,
    counters: &NetworkCounters,
    budget: &crate::ByteBudget,
) -> Result<(), ClientError> {
    if line.trim().is_empty() {
        return Ok(());
    }

    let reservation = budget
        .reserve(line.len())
        .await
        .ok_or(ClientError::FrameTooLarge)?;
    let event = match serde_json::from_str(&line) {
        Ok(event) => event,

        Err(_) => {
            counters.register_malformed_event();

            return Ok(());
        }
    };

    drop(line);
    sender
        .send_async(crate::ingestion::AdmittedEvent::new(event, reservation))
        .await
        .map_err(|_| ClientError::QueueClosed)
}
