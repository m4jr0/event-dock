use clap::{Parser, Subcommand, ValueEnum};
use eventdock::{EventStore, IncomingEvent, LogLevel, Logger, TcpSink};
use std::{error::Error, io, net::SocketAddr};
use tokio::{
    io::{AsyncBufReadExt, BufReader},
    net::{TcpListener, TcpStream},
    sync::mpsc,
};

type AppResult<T> = Result<T, Box<dyn Error + Send + Sync>>;
const DEFAULT_ADDRESS: &str = "127.0.0.1:7878";
const DEFAULT_STORE_CAPACITY: usize = 100_000;
const INGESTION_QUEUE_CAPACITY: usize = 4_096;

#[derive(Debug, Parser)]
#[command(name = "eventdock")]
#[command(about = "Receive, inspect, and send structured application events")]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, Subcommand)]
enum Command {
    Serve {
        #[arg(long, default_value = DEFAULT_ADDRESS)]
        address: String,
        #[arg(long, default_value_t = DEFAULT_STORE_CAPACITY)]
        capacity: usize,
    },
    Send {
        #[arg(long, default_value = DEFAULT_ADDRESS)]
        address: String,
        #[arg(long, default_value = "app")]
        source: String,
        #[arg(long, value_enum, default_value_t = CliLogLevel::Info)]
        level: CliLogLevel,
        #[arg(long)]
        message: String,
        #[arg(long = "field", value_name = "KEY=VALUE")]
        fields: Vec<String>,
    },
}

#[derive(Debug, Clone, Copy, ValueEnum)]
enum CliLogLevel {
    Trace,
    Debug,
    Info,
    Warning,
    Error,
    Critical,
}

impl From<CliLogLevel> for LogLevel {
    fn from(level: CliLogLevel) -> Self {
        match level {
            CliLogLevel::Trace => Self::Trace,
            CliLogLevel::Debug => Self::Debug,
            CliLogLevel::Info => Self::Info,
            CliLogLevel::Warning => Self::Warning,
            CliLogLevel::Error => Self::Error,
            CliLogLevel::Critical => Self::Critical,
        }
    }
}

#[tokio::main]
async fn main() -> AppResult<()> {
    let cli = Cli::parse();
    match cli.command {
        Command::Serve { address, capacity } => {
            serve(&address, capacity).await?
        }
        Command::Send {
            address,
            source,
            level,
            message,
            fields,
        } => {
            send(&address, source, level.into(), message, fields)?;
        }
    }
    Ok(())
}

async fn serve(
    address: &str,
    capacity: usize,
) -> AppResult<()> {
    if capacity == 0 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "store capacity must be greater than zero",
        )
        .into());
    }
    let listener = TcpListener::bind(address).await?;
    let (event_sender, mut event_receiver) =
        mpsc::channel::<IncomingEvent>(INGESTION_QUEUE_CAPACITY);
    println!("⚓ EventDock listening on {address}");
    println!("   Store capacity: {capacity} events");
    println!("   Queue capacity: {INGESTION_QUEUE_CAPACITY} events");
    tokio::spawn(async move {
        let mut store = EventStore::new(capacity);
        while let Some(event) = event_receiver.recv().await {
            let source = event.source.clone();
            let level = event.level;
            let message = event.message.clone();
            let id = store.push(event);
            println!("#{id} [{level}] [{source}] {message}");
        }
    });
    loop {
        let (stream, peer_address) = listener.accept().await?;
        println!("📡 Client connected: {peer_address}");
        let client_sender = event_sender.clone();
        tokio::spawn(async move {
            if let Err(error) =
                handle_client(stream, peer_address, client_sender).await
            {
                eprintln!("Client {peer_address} failed: {error}");
            }
        });
    }
}

async fn handle_client(
    stream: TcpStream,
    peer_address: SocketAddr,
    event_sender: mpsc::Sender<IncomingEvent>,
) -> AppResult<()> {
    let reader = BufReader::new(stream);
    let mut lines = reader.lines();
    while let Some(line) = lines.next_line().await? {
        if line.trim().is_empty() {
            continue;
        }
        let event: IncomingEvent = match serde_json::from_str(&line) {
            Ok(event) => event,
            Err(error) => {
                eprintln!(
                    "Rejected malformed event from {peer_address}: {error}"
                );
                continue;
            }
        };
        event_sender
            .send(event)
            .await
            .map_err(|_| {
                io::Error::new(
                    io::ErrorKind::BrokenPipe,
                    "the EventDock ingestion task stopped",
                )
            })?;
    }
    println!("📴 Client disconnected: {peer_address}");
    Ok(())
}

fn send(
    address: &str,
    source: String,
    level: LogLevel,
    message: String,
    fields: Vec<String>,
) -> AppResult<()> {
    let mut event = IncomingEvent::new(source.clone(), level, message);
    for field in fields {
        let (name, value) = parse_field_argument(&field)?;
        event.insert_field(name, value);
    }
    let sink = TcpSink::connect(address)?;
    let mut logger = Logger::new(source, sink);
    logger.log_event(event)?;
    println!("🚢 Event sent to {address}");
    Ok(())
}

fn parse_field_argument(field: &str) -> AppResult<(String, String)> {
    let (name, value) = field.split_once('=').ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("invalid field '{field}'; expected KEY=VALUE"),
        )
    })?;
    if name.is_empty() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "field names cannot be empty",
        )
        .into());
    }
    Ok((name.to_owned(), value.to_owned()))
}
