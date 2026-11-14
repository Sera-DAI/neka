//! Storage: reading and writing the `.neka` file (the format specification),
//! opening by password or recovery key, password change and atomic
//! write.

mod atomic;
mod error;
mod file;
mod vault;

pub use atomic::{backup, read_file, write_atomic, Step};
pub use error::StorageError;
pub use file::{File, Slot, CURRENT_VERSION, MAGIC, MAX_FILE_LEN};
pub use vault::OpenVault;

#[doc(hidden)]
pub use atomic::write_atomic_with_failure;
