//! Application layer: the only entry point for the interfaces
//! (`neka-cli`, and `neka-android` in the future). High-level operations on
//! a vault, with errors as typed codes and static text.
//!
//! Time (`now`, ms since 1970 UTC) and the device identifier come in
//! as parameters: the clock and the device storage belong to the interface.

mod error;
mod import;
mod recovery;
mod sync;
mod vault;

pub use crate::io::{
    export_csv as generate_csv, export_kdbx as generate_kdbx, import_csv as read_csv,
    import_kdbx as read_kdbx, IoError, Record,
};
pub use error::ApiError;
pub use import::ImportReport;
pub use recovery::{from_text as recovery_key_from_text, to_text as recovery_key_to_text};
pub use sync::{SyncReport, BACKUPS_KEPT};
pub use vault::{Credential, NewEntry, Vault};

// Types the interfaces need to see without touching the lower layers.
pub use crate::crypto::{
    generate_password, Classes, KdfParams, Key, PASSWORD_LEN_MAX, PASSWORD_LEN_MIN,
};
pub use crate::model::{Entry, Uuid};

/// Core version, for the interface to show in `--version`.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");

/// Random identifier for a new device (the interface stores it).
pub fn new_device_id() -> Result<Uuid, ApiError> {
    let mut b = [0u8; 16];
    crate::crypto::random(&mut b)?;
    Ok(uuid::Builder::from_random_bytes(b).into_uuid())
}
