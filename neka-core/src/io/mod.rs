//! `io` layer: import and export (CSV and KeePass KDBX 4).
//!
//! Only converts bytes from/to [`Record`]. The API decides what goes into the vault.
//! Nothing here writes a file: the interface chooses where and warns that
//! a CSV export is **plaintext**.

mod csv;
mod error;
mod kdbx;

use std::collections::BTreeMap;
use std::fmt;

use zeroize::Zeroize;

pub use self::csv::{export_csv, import_csv};
pub use error::IoError;
pub use kdbx::{export_kdbx, import_kdbx};

/// An account to import or export, without the vault metadata.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Record {
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub extra: BTreeMap<String, String>,
}

impl fmt::Debug for Record {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Record { <hidden> }")
    }
}

impl Drop for Record {
    fn drop(&mut self) {
        self.title.zeroize();
        self.username.zeroize();
        self.password.zeroize();
        self.url.zeroize();
        self.notes.zeroize();
        for v in self.extra.values_mut() {
            v.zeroize();
        }
    }
}
