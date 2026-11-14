use core::fmt;

use crate::crypto::CryptoError;
use crate::model::ModelError;

/// Storage errors. Static text; I/O errors keep only the kind.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum StorageError {
    /// Does not start with `NEKA` or the structure is broken.
    InvalidFormat,
    /// File of a newer version than this program knows.
    NewerVersion,
    /// Old version with no known migration.
    UnsupportedVersion,
    /// `flags` with bits this program does not know.
    UnknownFlags,
    /// File larger than the accepted limit.
    FileTooLarge,
    /// There is no slot of the requested kind.
    NoCompartment,
    Crypto(CryptoError),
    Model(ModelError),
    Io(std::io::ErrorKind),
}

impl fmt::Display for StorageError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::InvalidFormat => f.write_str("file is not a valid neka vault"),
            Self::NewerVersion => {
                f.write_str("vault created by a newer version of neka; update the program")
            }
            Self::UnsupportedVersion => f.write_str("unsupported vault version"),
            Self::UnknownFlags => f.write_str("vault uses unknown features"),
            Self::FileTooLarge => f.write_str("file too large to be a vault"),
            Self::NoCompartment => f.write_str("vault has no such unlock method"),
            Self::Crypto(e) => fmt::Display::fmt(e, f),
            Self::Model(e) => fmt::Display::fmt(e, f),
            Self::Io(_) => f.write_str("file read or write failure"),
        }
    }
}

impl std::error::Error for StorageError {}

impl From<CryptoError> for StorageError {
    fn from(e: CryptoError) -> Self {
        Self::Crypto(e)
    }
}

impl From<ModelError> for StorageError {
    fn from(e: ModelError) -> Self {
        Self::Model(e)
    }
}

impl From<std::io::Error> for StorageError {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.kind())
    }
}
