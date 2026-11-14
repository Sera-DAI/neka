use core::fmt;

/// Import and export errors. Static text, no content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum IoError {
    /// Malformed CSV or no recognizable header.
    InvalidCsv,
    /// The CSV has neither a title column nor an address column.
    CsvNoColumns,
    /// Wrong password or corrupted KDBX file.
    KdbxOpenFailed,
    /// KeePass version we cannot read.
    KdbxUnsupported,
    /// Failed to generate the KDBX.
    KdbxWriteFailed,
}

impl fmt::Display for IoError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::InvalidCsv => "invalid CSV file",
            Self::CsvNoColumns => "the CSV has no title, username, password or address columns",
            Self::KdbxOpenFailed => "wrong password or corrupted KeePass file",
            Self::KdbxUnsupported => "unsupported KeePass file version",
            Self::KdbxWriteFailed => "failed to generate the KeePass file",
        })
    }
}

impl std::error::Error for IoError {}
