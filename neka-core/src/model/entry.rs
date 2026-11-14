use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

use super::ModelError;

/// Maximum old passwords kept per entry (the format specification).
pub const MAX_HISTORY: usize = 10;

const MAX_TITLE: usize = 256;
const MAX_USERNAME: usize = 256;
const MAX_PASSWORD: usize = 4096;
const MAX_URL: usize = 2048;
const MAX_NOTES: usize = 64 * 1024;
const MAX_TAGS: usize = 64;
const MAX_TAG: usize = 64;
const MAX_EXTRAS: usize = 64;
const MAX_EXTRA_NAME: usize = 64;
const MAX_EXTRA_VALUE: usize = 4096;

/// Previous password and the instant (ms since 1970, UTC) it was changed.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct OldPassword {
    pub password: String,
    pub changed_at: i64,
}

impl fmt::Debug for OldPassword {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SenhaAntiga")
            .field("password", &"<hidden>")
            .field("changed_at", &self.changed_at)
            .finish()
    }
}

impl Drop for OldPassword {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

/// Vault entry. Field names follow the format specification.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entry {
    pub id: Uuid,
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub extra: BTreeMap<String, String>,
    pub history: Vec<OldPassword>,
    pub created_at: i64,
    pub updated_at: i64,
    pub clock: u64,
    pub device_id: Uuid,
}

/// `Debug` hides title, password, notes and extra fields.
impl fmt::Debug for Entry {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entry")
            .field("id", &self.id)
            .field("clock", &self.clock)
            .field("device_id", &self.device_id)
            .field("secrets", &"<hidden>")
            .finish()
    }
}

impl Drop for Entry {
    fn drop(&mut self) {
        self.title.zeroize();
        self.username.zeroize();
        self.password.zeroize();
        self.url.zeroize();
        self.notes.zeroize();
        for t in &mut self.tags {
            t.zeroize();
        }
        for v in self.extra.values_mut() {
            v.zeroize();
        }
    }
}

impl Entry {
    /// Creates a new entry. `now` in ms since 1970 (UTC).
    pub fn new(id: Uuid, device_id: Uuid, title: String, now: i64, clock: u64) -> Self {
        Self {
            id,
            title,
            username: String::new(),
            password: String::new(),
            url: String::new(),
            notes: String::new(),
            tags: Vec::new(),
            extra: BTreeMap::new(),
            history: Vec::new(),
            created_at: now,
            updated_at: now,
            clock,
            device_id,
        }
    }

    /// Changes the password, keeping the previous one in the history (at most
    /// [`MAX_HISTORY`]; the oldest leaves first).
    pub fn change_password(&mut self, new: String, now: i64) {
        let old = std::mem::replace(&mut self.password, new);
        if !old.is_empty() {
            self.history.push(OldPassword {
                password: old,
                changed_at: now,
            });
            while self.history.len() > MAX_HISTORY {
                self.history.remove(0);
            }
        }
        self.updated_at = now;
    }

    /// Checks the size limits and the format of the fields.
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.title.trim().is_empty() {
            return Err(ModelError::EmptyTitle);
        }
        if self.title.len() > MAX_TITLE {
            return Err(ModelError::TitleTooLong);
        }
        if self.username.len() > MAX_USERNAME {
            return Err(ModelError::UsernameTooLong);
        }
        if self.password.len() > MAX_PASSWORD {
            return Err(ModelError::PasswordTooLong);
        }
        if self.url.len() > MAX_URL {
            return Err(ModelError::UrlTooLong);
        }
        if self.notes.len() > MAX_NOTES {
            return Err(ModelError::NotesTooLong);
        }
        if self.tags.len() > MAX_TAGS {
            return Err(ModelError::TooManyTags);
        }
        if self.tags.iter().any(|t| t.is_empty() || t.len() > MAX_TAG) {
            return Err(ModelError::InvalidTag);
        }
        if self.extra.len() > MAX_EXTRAS {
            return Err(ModelError::TooManyExtras);
        }
        if self
            .extra
            .iter()
            .any(|(k, v)| k.is_empty() || k.len() > MAX_EXTRA_NAME || v.len() > MAX_EXTRA_VALUE)
        {
            return Err(ModelError::InvalidExtra);
        }
        if self.history.len() > MAX_HISTORY
            || self.history.iter().any(|h| h.password.len() > MAX_PASSWORD)
        {
            return Err(ModelError::HistoryTooLong);
        }
        Ok(())
    }
}

/// Deletion marker. Holds no field of the original entry.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Tombstone {
    pub id: Uuid,
    pub deleted_at: i64,
    pub clock: u64,
    pub device_id: Uuid,
}
