use serde::{Serialize, de::DeserializeOwned};
use std::io::{self, BufRead, Write};

pub const MAX_REQUEST_BYTES: usize = 4096;
pub const MAX_RESPONSE_BYTES: usize = 4 * 1024 * 1024;

/// Read one complete JSON frame, including its terminating newline in the limit.
pub fn read_json_line<T: DeserializeOwned>(
    reader: impl BufRead,
    limit: usize,
) -> io::Result<T> {
    let mut bytes = Vec::new();
    reader
        .take(limit.saturating_add(1) as u64)
        .read_until(b'\n', &mut bytes)?;
    if bytes.len() > limit || !bytes.ends_with(b"\n") {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            "invalid query frame length or missing newline",
        ));
    }
    serde_json::from_slice(&bytes)
        .map_err(|error| io::Error::new(io::ErrorKind::InvalidData, error))
}

struct BoundedBuffer {
    bytes: Vec<u8>,
    limit: usize,
}

impl Write for BoundedBuffer {
    fn write(
        &mut self,
        bytes: &[u8],
    ) -> io::Result<usize> {
        if bytes.len()
            > self
                .limit
                .saturating_sub(self.bytes.len())
        {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                "query frame exceeds wire limit",
            ));
        }
        self.bytes.extend_from_slice(bytes);
        Ok(bytes.len())
    }

    fn flush(&mut self) -> io::Result<()> {
        Ok(())
    }
}

/// Encode before transmission so oversized frames never reach the socket.
pub fn encode_json_line<T: Serialize>(
    value: &T,
    limit: usize,
) -> io::Result<Vec<u8>> {
    let mut buffer = BoundedBuffer {
        bytes: Vec::new(),
        limit,
    };
    serde_json::to_writer(&mut buffer, value).map_err(|error| {
        io::Error::new(
            error
                .io_error_kind()
                .unwrap_or(io::ErrorKind::InvalidData),
            error,
        )
    })?;
    buffer.write_all(b"\n")?;
    Ok(buffer.bytes)
}

pub fn write_json_line<T: Serialize>(
    writer: &mut impl Write,
    value: &T,
    limit: usize,
) -> io::Result<()> {
    writer.write_all(&encode_json_line(value, limit)?)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn frame_limit_includes_escaping_and_newline() {
        let value = "\n";
        let bytes = encode_json_line(&value, 5).unwrap();
        assert_eq!(bytes.len(), 5);
        assert_eq!(
            read_json_line::<String>(bytes.as_slice(), 5).unwrap(),
            value
        );
        assert!(encode_json_line(&value, 4).is_err());
        assert!(read_json_line::<String>(bytes.as_slice(), 4).is_err());
        assert!(read_json_line::<String>(&bytes[..4], 5).is_err());
    }
}
