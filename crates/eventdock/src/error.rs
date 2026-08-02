use std::{error::Error, fmt, io};

#[derive(Debug)]
pub enum EventDockError {
    Io(io::Error),
    Serialization(serde_json::Error),
}

impl fmt::Display for EventDockError {
    fn fmt(
        &self,
        f: &mut fmt::Formatter<'_>,
    ) -> fmt::Result {
        match self {
            Self::Io(error) => write!(f, "I/O error: {error}"),
            Self::Serialization(error) => {
                write!(f, "serialization error: {error}")
            }
        }
    }
}

impl Error for EventDockError {
    fn source(&self) -> Option<&(dyn Error + 'static)> {
        match self {
            Self::Io(error) => Some(error),
            Self::Serialization(error) => Some(error),
        }
    }
}

impl From<io::Error> for EventDockError {
    fn from(error: io::Error) -> Self {
        Self::Io(error)
    }
}

impl From<serde_json::Error> for EventDockError {
    fn from(error: serde_json::Error) -> Self {
        Self::Serialization(error)
    }
}
