//! Sync through a cloud folder (D3, D11, D12, D15).
//!
//! The local vault is each device's source of truth. The folder (for
//! example, inside Google Drive) only receives complete copies, written
//! atomically. The sync:
//!
//! 1. makes a dated backup of the local vault;
//! 2. reads every `*.neka` in the folder with the same `vault_id` (including
//!    the conflict copies the cloud service creates);
//! 3. merges each into the local vault (D11). A file with a higher key epoch
//!    (password changed on another device) becomes the base, and the
//!    local vault adopts the new key. A file with a lower epoch is an old
//!    copy: neither merged nor deleted (D15);
//! 4. removes tombstones older than 180 days (D12);
//! 5. writes the local vault and publishes the copy in the folder;
//! 6. deletes the conflict copies already absorbed.

use std::fs;
use std::path::{Path, PathBuf};

use super::{ApiError, Vault};
use crate::store::{read_file, write_atomic, File, OpenVault, StorageError};
use crate::sync::{merge, prune_tombstones};

/// How many dated backups to keep per vault.
pub const BACKUPS_KEPT: usize = 20;

/// What the sync did. Paths and counts only, none of the content.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct SyncReport {
    /// Files in the folder merged into the local vault.
    pub merged: Vec<PathBuf>,
    /// Conflict copies deleted after being absorbed.
    pub removed: Vec<PathBuf>,
    /// Files with a lower epoch (old password): left as they are.
    pub old_copies: Vec<PathBuf>,
    /// Files with the same epoch that the password cannot open (concurrent
    /// password change on two devices): left as they are.
    pub not_open: Vec<PathBuf>,
    /// Files of another vault or that are not valid vaults: ignored.
    pub ignored: Vec<PathBuf>,
    /// Tombstones older than 180 days removed.
    pub pruned_tombstones: usize,
    /// The local vault adopted the key of a file with a newer password.
    /// The master password and the recovery key become that file's.
    pub adopted_new_password: bool,
    /// Whether the local content changed with the merge.
    pub changed_locally: bool,
    /// Backup made before touching anything.
    pub backup: PathBuf,
    /// Copy published in the folder.
    pub published: PathBuf,
}

fn is_vault(p: &Path) -> bool {
    p.is_file()
        && p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("neka"))
}

fn io(e: std::io::Error) -> ApiError {
    ApiError::Storage(StorageError::Io(e.kind()))
}

/// Dated backup in `<vault folder>/backups/`, keeping the most recent
/// [`BACKUPS_KEPT`] ones.
fn dated_backup(path: &Path, now: i64) -> Result<PathBuf, ApiError> {
    let folder = path.parent().unwrap_or(Path::new(".")).join("backups");
    fs::create_dir_all(&folder).map_err(io)?;
    let base = path
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("vault")
        .to_owned();
    let target = folder.join(format!("{base}-{now:015}.neka"));
    write_atomic(&target, &read_file(path)?)?;

    let prefix = format!("{base}-");
    let mut old: Vec<PathBuf> = fs::read_dir(&folder)
        .map_err(io)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            is_vault(p)
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(&prefix))
        })
        .collect();
    old.sort();
    let leftover = old.len().saturating_sub(BACKUPS_KEPT);
    for p in &old[..leftover] {
        let _ = fs::remove_file(p);
    }
    Ok(target)
}

impl Vault {
    /// Syncs with the folder `folder`. `password` is the master password that opened
    /// this vault. If another device changed the password, the call fails with
    /// [`ApiError::PasswordChangedOnOtherDevice`] without changing anything; the
    /// interface asks for the new password and calls again with `new_password`.
    pub fn sync(
        &mut self,
        folder: &Path,
        password: &[u8],
        new_password: Option<&[u8]>,
        now: i64,
    ) -> Result<SyncReport, ApiError> {
        let name = self
            .path
            .file_name()
            .ok_or(ApiError::InvalidSyncFolder)?
            .to_owned();
        let published = folder.join(&name);
        if !folder.is_dir() {
            return Err(ApiError::InvalidSyncFolder);
        }
        let same = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
            (Ok(x), Ok(y)) => x == y,
            _ => false,
        };
        if same(folder, self.path.parent().unwrap_or(Path::new("."))) {
            return Err(ApiError::InvalidSyncFolder);
        }

        // Reads and classifies the folder's files before changing anything.
        let mut candidates: Vec<(PathBuf, u32, Vec<u8>)> = Vec::new();
        let mut rel = SyncReport {
            published: published.clone(),
            ..SyncReport::default()
        };
        let mut read: Vec<PathBuf> = fs::read_dir(folder)
            .map_err(io)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| is_vault(p))
            .collect();
        read.sort();
        for p in read {
            let Ok(bytes) = read_file(&p) else {
                rel.ignored.push(p);
                continue;
            };
            match File::decode(&bytes) {
                Ok(a) if a.vault_id == self.vault_id() => {
                    if a.key_epoch < self.key_epoch_value() {
                        rel.old_copies.push(p);
                    } else {
                        candidates.push((p, a.key_epoch, bytes));
                    }
                }
                _ => rel.ignored.push(p),
            }
        }
        // Higher epoch first: if there is a new password, it becomes the base.
        candidates.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        // Opens everything before changing the vault: if the new password is missing, nothing changes.
        let mut open: Vec<(PathBuf, OpenVault)> = Vec::new();
        let local_epoch = self.key_epoch_value();
        for (p, epoch, bytes) in candidates {
            let attempt = match new_password {
                Some(n) if epoch > local_epoch => OpenVault::open_with_password(&bytes, n),
                _ => OpenVault::open_with_password(&bytes, password),
            };
            match attempt {
                Ok(c) => open.push((p, c)),
                Err(_) if epoch > local_epoch => {
                    return Err(ApiError::PasswordChangedOnOtherDevice)
                }
                Err(_) => rel.not_open.push(p),
            }
        }

        rel.backup = dated_backup(&self.path, now)?;

        for (p, remote) in open {
            let merged = merge(self.open.body(), remote.body());
            if remote.key_epoch_value() > self.key_epoch_value() {
                let mut base = remote;
                *base.body_mut() = merged;
                self.open = base;
                rel.adopted_new_password = true;
                rel.changed_locally = true;
            } else if &merged != self.open.body() {
                *self.open.body_mut() = merged;
                rel.changed_locally = true;
            }
            rel.merged.push(p);
        }

        let mut body = self.open.body().clone();
        rel.pruned_tombstones = prune_tombstones(&mut body, now);
        if rel.pruned_tombstones > 0 {
            *self.open.body_mut() = body;
            rel.changed_locally = true;
        }

        let bytes = self.open.serialize()?;
        write_atomic(&self.path, &bytes)?;
        if read_file(&published).ok().as_deref() != Some(bytes.as_slice()) {
            write_atomic(&published, &bytes)?;
        }

        for p in &rel.merged {
            if p.file_name() != Some(name.as_os_str()) && fs::remove_file(p).is_ok() {
                rel.removed.push(p.clone());
            }
        }
        Ok(rel)
    }
}

impl Vault {
    /// Brings to this device a vault that already exists (for example, the
    /// copy in the Google Drive folder): checks that the credential opens the
    /// file and writes the local copy to `target`, which must not exist.
    pub fn clone(
        source: &Path,
        target: &Path,
        device_id: crate::model::Uuid,
        password: &[u8],
    ) -> Result<Self, ApiError> {
        let bytes = read_file(source)?;
        OpenVault::open_with_password(&bytes, password)?;
        if target.exists() {
            return Err(ApiError::FileAlreadyExists);
        }
        if let Some(p) = target.parent() {
            fs::create_dir_all(p).map_err(io)?;
        }
        write_atomic(target, &bytes)?;
        Vault::open(target, device_id, &super::Credential::Password(password))
    }
}
