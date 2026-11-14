//! Import and export at the application level.

use super::{ApiError, NewEntry, Vault};
use crate::io::Record;

/// Result of an import. Counts only.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct ImportReport {
    pub added: usize,
    /// Already existed (same title, username, address and password).
    pub duplicates: usize,
    /// Refused by validation (a field too large, for example).
    pub invalid: usize,
}

fn same_account(r: &Record, e: &crate::model::Entry) -> bool {
    r.title.trim().eq_ignore_ascii_case(e.title.trim())
        && r.username == e.username
        && r.url == e.url
        && r.password == e.password
}

impl Vault {
    /// Adds the records as new entries. Does not write: call
    /// [`Vault::save`] afterwards. If nothing is valid, the vault does not change.
    pub fn import(&mut self, records: &[Record], now: i64) -> Result<ImportReport, ApiError> {
        let mut rel = ImportReport::default();
        for r in records {
            if self.entries().iter().any(|e| same_account(r, e)) {
                rel.duplicates += 1;
                continue;
            }
            let new = NewEntry {
                title: r.title.clone(),
                username: r.username.clone(),
                password: r.password.clone(),
                url: r.url.clone(),
                notes: r.notes.clone(),
                tags: r.tags.clone(),
                extra: r.extra.clone(),
            };
            match self.add(new, now) {
                Ok(_) => rel.added += 1,
                Err(ApiError::Model(_)) => rel.invalid += 1,
                Err(e) => return Err(e),
            }
        }
        Ok(rel)
    }

    /// All entries as records, for export.
    pub fn records(&self) -> Vec<Record> {
        self.entries()
            .iter()
            .map(|e| Record {
                title: e.title.clone(),
                username: e.username.clone(),
                password: e.password.clone(),
                url: e.url.clone(),
                notes: e.notes.clone(),
                tags: e.tags.clone(),
                extra: e.extra.clone(),
            })
            .collect()
    }
}
