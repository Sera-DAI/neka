use core::fmt;

/// Domain errors. Static text: never includes vault content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ModelError {
    EmptyTitle,
    TitleTooLong,
    UsernameTooLong,
    PasswordTooLong,
    UrlTooLong,
    NotesTooLong,
    TooManyTags,
    InvalidTag,
    TooManyExtras,
    InvalidExtra,
    HistoryTooLong,
    DuplicateId,
    UnknownSchema,
    InvalidCbor,
    CborNotSerializable,
}

impl fmt::Display for ModelError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::EmptyTitle => "empty title",
            Self::TitleTooLong => "title too long",
            Self::UsernameTooLong => "username too long",
            Self::PasswordTooLong => "password too long",
            Self::UrlTooLong => "address too long",
            Self::NotesTooLong => "notes too long",
            Self::TooManyTags => "too many tags",
            Self::InvalidTag => "empty or too long tag",
            Self::TooManyExtras => "too many extra fields",
            Self::InvalidExtra => "extra field with empty name or size exceeded",
            Self::HistoryTooLong => "password history too long",
            Self::DuplicateId => "duplicate identifier in the vault",
            Self::UnknownSchema => "unknown vault body schema version",
            Self::InvalidCbor => "invalid vault body",
            Self::CborNotSerializable => "failed to serialize the vault body",
        })
    }
}

impl std::error::Error for ModelError {}
