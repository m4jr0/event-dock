//! Errors from retaining events in a storage backend.

use crate::CompactEncodingError;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageError {
    Encoding(CompactEncodingError),
    IdExhausted,
    PayloadTooLarge,
}

impl From<CompactEncodingError> for StorageError {
    fn from(error: CompactEncodingError) -> Self {
        Self::Encoding(error)
    }
}
