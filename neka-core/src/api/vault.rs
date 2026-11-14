use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use super::{recovery_key_from_text, ApiError};
use crate::crypto::{random, KdfParams, Key};
use crate::model::{Body, Entry, Tombstone, Uuid};
use crate::store::{read_file, write_atomic, OpenVault};

/// How to unlock the vault. Secrets are borrowed: the interface is
/// the one that zeroes its own buffers.
pub enum Credential<'a> {
    Password(&'a [u8]),
    /// Recovery key text, as shown at creation.
    Recovery(&'a str),
}

/// Fields of a new entry.
#[derive(Default)]
pub struct NewEntry {
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub extra: std::collections::BTreeMap<String, String>,
}

/// Open vault, tied to the local file it came from. Changes stay
/// in memory until [`Vault::save`].
pub struct Vault {
    pub(super) open: OpenVault,
    pub(super) path: PathBuf,
    device_id: Uuid,
    /// Exclusive lock of `<vault>.lock`, held while the vault is
    /// open: prevents two instances from saving at the same time.
    _lock: File,
}

/// Takes the vault's exclusive lock, without waiting.
fn lock(path: &Path) -> Result<File, ApiError> {
    let mut name = path.as_os_str().to_owned();
    name.push(".lock");
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(PathBuf::from(name))
        .map_err(|e| ApiError::Storage(e.into()))?;
    match f.try_lock() {
        Ok(()) => Ok(f),
        Err(TryLockError::WouldBlock) => Err(ApiError::VaultInUse),
        Err(TryLockError::Error(e)) => Err(ApiError::Storage(e.into())),
    }
}

impl std::fmt::Debug for Vault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Vault")
            .field("path", &self.path)
            .finish_non_exhaustive()
    }
}

fn random_id() -> Result<Uuid, ApiError> {
    let mut b = [0u8; 16];
    random(&mut b)?;
    Ok(uuid::Builder::from_random_bytes(b).into_uuid())
}

impl Vault {
    /// Creates and writes a new vault at `path` (which must not exist).
    /// Returns the vault and the recovery key, to be shown exactly once
    /// (see `recovery_key_to_text`).
    pub fn create(
        path: &Path,
        device_id: Uuid,
        password: &[u8],
        kdf: KdfParams,
    ) -> Result<(Self, Key), ApiError> {
        if password.is_empty() {
            return Err(ApiError::EmptyPassword);
        }
        kdf.validate()?;
        let lock = lock(path)?;
        if path.exists() {
            return Err(ApiError::FileAlreadyExists);
        }
        let (open, recovery) = OpenVault::create(password, kdf, Body::default())?;
        let mut c = Self {
            open,
            path: path.to_owned(),
            device_id,
            _lock: lock,
        };
        c.save()?;
        Ok((c, recovery))
    }

    pub fn open(
        path: &Path,
        device_id: Uuid,
        credential: &Credential<'_>,
    ) -> Result<Self, ApiError> {
        let lock = lock(path)?;
        let bytes = read_file(path)?;
        let open = match credential {
            Credential::Password(s) => OpenVault::open_with_password(&bytes, s)?,
            Credential::Recovery(t) => {
                OpenVault::open_with_recovery(&bytes, &recovery_key_from_text(t)?)?
            }
        };
        Ok(Self {
            open,
            path: path.to_owned(),
            device_id,
            _lock: lock,
        })
    }

    /// Writes to the vault path, atomically.
    pub fn save(&mut self) -> Result<(), ApiError> {
        let bytes = self.open.serialize()?;
        write_atomic(&self.path, &bytes)?;
        Ok(())
    }

    pub fn vault_id(&self) -> Uuid {
        self.open.vault_id()
    }

    pub fn entries(&self) -> &[Entry] {
        &self.open.body().entries
    }

    pub fn entry(&self, id: Uuid) -> Result<&Entry, ApiError> {
        self.entries()
            .iter()
            .find(|e| e.id == id)
            .ok_or(ApiError::EntryNotFound)
    }

    fn next_clock(&mut self) -> u64 {
        let body = self.open.body_mut();
        body.device_clock = body.device_clock.saturating_add(1);
        body.device_clock
    }

    pub fn add(&mut self, new: NewEntry, now: i64) -> Result<Uuid, ApiError> {
        let id = random_id()?;
        let mut e = Entry::new(id, self.device_id, new.title, now, 0);
        e.username = new.username;
        e.password = new.password;
        e.url = new.url;
        e.notes = new.notes;
        e.tags = new.tags;
        e.extra = new.extra;
        e.validate()?;
        e.clock = self.next_clock();
        self.open.body_mut().entries.push(e);
        Ok(id)
    }

    /// Edits an entry. A password change goes to the history. The `id` must
    /// not change. Nothing is altered if validation fails.
    pub fn edit(
        &mut self,
        id: Uuid,
        change: impl FnOnce(&mut Entry),
        now: i64,
    ) -> Result<(), ApiError> {
        let current = self.entry(id)?;
        let mut copy = current.clone();
        change(&mut copy);
        copy.id = id;
        if copy.password != current.password {
            let new = std::mem::replace(&mut copy.password, current.password.clone());
            copy.history = current.history.clone();
            copy.change_password(new, now);
        }
        copy.updated_at = now;
        copy.created_at = current.created_at;
        copy.device_id = self.device_id;
        copy.validate()?;
        copy.clock = self.next_clock();
        let body = self.open.body_mut();
        let pos = body
            .entries
            .iter()
            .position(|e| e.id == id)
            .ok_or(ApiError::EntryNotFound)?;
        body.entries[pos] = copy;
        Ok(())
    }

    /// Removes the entry and leaves a tombstone (so the deletion propagates).
    pub fn remove(&mut self, id: Uuid, now: i64) -> Result<(), ApiError> {
        self.entry(id)?;
        let clock = self.next_clock();
        let device_id = self.device_id;
        let body = self.open.body_mut();
        body.entries.retain(|e| e.id != id);
        body.tombstones.push(Tombstone {
            id,
            deleted_at: now,
            clock,
            device_id,
        });
        Ok(())
    }

    /// Changes the master password **and rotates the data key** (D15): the old password
    /// and the old recovery key stop opening the new file, which is
    /// re-encrypted entirely. Returns the **new recovery key**, to be shown
    /// exactly once (the old one stops being valid). Does not write: call
    /// [`Vault::save`] right afterwards.
    ///
    /// Does not reach old copies of the file: whoever has the old password and an
    /// old copy can still read what was in it.
    pub fn change_master_password(&mut self, new: &[u8], kdf: KdfParams) -> Result<Key, ApiError> {
        if new.is_empty() {
            return Err(ApiError::EmptyPassword);
        }
        kdf.validate()?;
        Ok(self.open.change_password(new, kdf)?)
    }

    /// Data key generation: goes up with each master password change.
    pub fn key_epoch_value(&self) -> u32 {
        self.open.key_epoch_value()
    }
}
