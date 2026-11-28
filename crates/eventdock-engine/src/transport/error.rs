use std::io;
use tokio_util::codec::LinesCodecError;

#[derive(Debug)]
pub(crate) enum ClientError {
    Io(io::Error),
    FrameTooLarge,
    InvalidEncoding,
    QueueClosed,
}

impl From<LinesCodecError> for ClientError {
    fn from(error: LinesCodecError) -> Self {
        match error {
            LinesCodecError::MaxLineLengthExceeded => Self::FrameTooLarge,

            LinesCodecError::Io(error)
                if error.kind() == io::ErrorKind::InvalidData =>
            {
                Self::InvalidEncoding
            }

            LinesCodecError::Io(error) => Self::Io(error),
        }
    }
}
