use core::fmt;

use crate::crypto::CryptoError;
use crate::model::ModelError;
use crate::store::StorageError;

/// API error. Each variant has a stable code (`code`) and a static text
/// (`Display`); none carries a password, key or vault content.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ApiError {
    /// A file already exists at that path; `neka` never overwrites vaults.
    FileAlreadyExists,
    EntryNotFound,
    /// Another process (or instance) has this vault open.
    VaultInUse,
    /// Recovery key text is malformed or has a typo.
    InvalidRecoveryKey,
    /// Empty master password.
    EmptyPassword,
    /// The sync folder does not exist or is the vault's own folder.
    InvalidSyncFolder,
    /// A file in the folder has a newer password (changed on another
    /// device): the new password must be typed.
    PasswordChangedOnOtherDevice,
    Storage(StorageError),
    Crypto(CryptoError),
    Model(ModelError),
}

impl ApiError {
    /// Stable code for interfaces and logs (never changes meaning).
    pub fn code(&self) -> &'static str {
        match self {
            Self::FileAlreadyExists => "file_already_exists",
            Self::EntryNotFound => "entry_not_found",
            Self::VaultInUse => "vault_in_use",
            Self::InvalidRecoveryKey => "invalid_recovery_key",
            Self::EmptyPassword => "empty_password",
            Self::InvalidSyncFolder => "invalid_sync_folder",
            Self::PasswordChangedOnOtherDevice => "password_changed_on_another_device",
            Self::Storage(StorageError::Crypto(CryptoError::AuthenticationFailed))
            | Self::Crypto(CryptoError::AuthenticationFailed) => {
                "incorrect_credential_or_tampered_file"
            }
            Self::Storage(StorageError::NewerVersion) => "newer_version",
            Self::Storage(StorageError::Io(_)) => "file_failure",
            Self::Storage(_) => "invalid_vault",
            Self::Crypto(_) => "cryptographic_failure",
            Self::Model(_) => "invalid_data",
        }
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FileAlreadyExists => f.write_str("a file already exists at that path"),
            Self::EntryNotFound => f.write_str("entry not found"),
            Self::VaultInUse => f.write_str("the vault is open in another process"),
            Self::InvalidRecoveryKey => f.write_str("invalid recovery key"),
            Self::EmptyPassword => f.write_str("the master password cannot be empty"),
            Self::InvalidSyncFolder => {
                f.write_str("sync folder does not exist or is the same as the vault folder")
            }
            Self::PasswordChangedOnOtherDevice => {
                f.write_str("the master password was changed on another device")
            }
            Self::Storage(e) => fmt::Display::fmt(e, f),
            Self::Crypto(e) => fmt::Display::fmt(e, f),
            Self::Model(e) => fmt::Display::fmt(e, f),
        }
    }
}

impl std::error::Error for ApiError {}

impl From<StorageError> for ApiError {
    fn from(e: StorageError) -> Self {
        Self::Storage(e)
    }
}
impl From<CryptoError> for ApiError {
    fn from(e: CryptoError) -> Self {
        Self::Crypto(e)
    }
}
impl From<ModelError> for ApiError {
    fn from(e: ModelError) -> Self {
        Self::Model(e)
    }
}
