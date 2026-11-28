use super::AppResult;
use crate::cli::QueryArgs;
use eventdock_client::{Client, QueryRequest, RemoteTransport};
use std::{io, net::ToSocketAddrs};

pub fn run(args: QueryArgs) -> AppResult<()> {
    let request = QueryRequest {
        limit: args.limit,
        before_id: args.before_id,
        level: args.level.map(Into::into),
    };
    let response = query(&args.address, request)?;
    println!("{}", serde_json::to_string_pretty(&response)?);
    Ok(())
}

// Try resolved addresses in order, but do not retry application-level errors.
fn query(
    address: &str,
    request: QueryRequest,
) -> io::Result<eventdock_client::QueryResponse> {
    let mut last_error = io::Error::new(
        io::ErrorKind::InvalidInput,
        "no query address resolved",
    );
    for address in address.to_socket_addrs()? {
        let client = Client::new(RemoteTransport::new(address, 1)?);
        match client.submit_query(request)?.recv() {
            Err(error)
                if matches!(
                    error.kind(),
                    io::ErrorKind::ConnectionRefused
                        | io::ErrorKind::AddrNotAvailable
                        | io::ErrorKind::NetworkUnreachable
                        | io::ErrorKind::HostUnreachable
                ) =>
            {
                last_error = error;
            }
            result => return result,
        }
    }
    Err(last_error)
}
