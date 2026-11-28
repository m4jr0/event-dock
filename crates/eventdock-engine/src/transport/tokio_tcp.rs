use super::{
    client::read_client, counters::NetworkCounters, error::ClientError,
};
use eventdock_core::IncomingEvent;
use flume::Sender;
use std::{io, sync::Arc};
use tokio::{
    net::{TcpListener, TcpStream},
    sync::oneshot,
    task::{JoinError, JoinSet},
};

#[derive(Clone, Copy)]
pub(crate) struct CollectorLimits {
    pub max_connections: usize,
    pub max_event_bytes: usize,
}

pub(crate) async fn collect(
    listener: TcpListener,
    sender: Sender<IncomingEvent>,
    mut stop: oneshot::Receiver<()>,
    counters: Arc<NetworkCounters>,
    limits: CollectorLimits,
) -> io::Result<()> {
    let mut clients = JoinSet::new();

    let result = run_accept_loop(
        &listener,
        &sender,
        &mut stop,
        &mut clients,
        &counters,
        limits,
    )
    .await;

    shutdown_clients(&mut clients, &counters).await;
    result
}

async fn run_accept_loop(
    listener: &TcpListener,
    sender: &Sender<IncomingEvent>,
    stop: &mut oneshot::Receiver<()>,
    clients: &mut JoinSet<Result<(), ClientError>>,
    counters: &Arc<NetworkCounters>,
    limits: CollectorLimits,
) -> io::Result<()> {
    loop {
        tokio::select! {
            _ = &mut *stop => return Ok(()),

            completed = clients.join_next(), if !clients.is_empty() => {
                if let Some(result) = completed {
                    handle_completed_client(result, counters);
                }
            }

            accepted = listener.accept() => {
                let (stream, _) = accepted?;
                accept_client(stream, sender, clients, counters, limits);
            }
        }
    }
}

fn accept_client(
    stream: TcpStream,
    sender: &Sender<IncomingEvent>,
    clients: &mut JoinSet<Result<(), ClientError>>,
    counters: &Arc<NetworkCounters>,
    limits: CollectorLimits,
) {
    // len() includes completed tasks not yet reaped. Reap opportunistically before admission.
    while let Some(result) = clients.try_join_next() {
        handle_completed_client(result, counters);
    }

    if clients.len() >= limits.max_connections {
        counters.register_refused_connection();
        return;
    }

    counters.register_connection();

    clients.spawn(read_client(
        stream,
        sender.clone(),
        Arc::clone(counters),
        limits.max_event_bytes,
    ));
}

fn handle_completed_client(
    result: Result<Result<(), ClientError>, JoinError>,
    counters: &NetworkCounters,
) {
    counters.register_disconnection();

    match result {
        Ok(Ok(())) => {
            counters.register_normal_disconnect();
        }

        Ok(Err(ClientError::FrameTooLarge)) => {
            counters.register_oversized_frame();
        }

        Ok(Err(ClientError::InvalidEncoding)) => {
            counters.register_invalid_encoding();
        }

        Ok(Err(ClientError::Io(error))) => {
            eprintln!("EventDock client I/O error: {error}");
            counters.register_io_error();
        }

        Ok(Err(ClientError::QueueClosed)) => {}

        Err(error) => {
            if error.is_panic() {
                counters.register_task_failure();
            }
        }
    }
}

async fn shutdown_clients(
    clients: &mut JoinSet<Result<(), ClientError>>,
    counters: &NetworkCounters,
) {
    clients.abort_all();

    while let Some(result) = clients.join_next().await {
        handle_completed_client(result, counters);
    }
}
