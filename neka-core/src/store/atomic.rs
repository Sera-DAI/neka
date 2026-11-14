//! Atomic write: temp file in the same folder, fsync, rename.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use super::{StorageError, MAX_FILE_LEN};

/// Steps of the write, to inject failures in tests.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Step {
    CreateTemp,
    Write,
    Sync,
    Rename,
    SyncFolder,
}

/// Temp file with an unpredictable name, in the same folder as the target.
fn temp(target: &Path) -> Result<PathBuf, StorageError> {
    let mut r = [0u8; 8];
    crate::crypto::random(&mut r)?;
    let hex: String = r.iter().map(|b| format!("{b:02x}")).collect();
    Ok(with_suffix(target, &format!(".{hex}.tmp")))
}

fn with_suffix(target: &Path, suffix: &str) -> PathBuf {
    let mut s = target.as_os_str().to_owned();
    s.push(suffix);
    PathBuf::from(s)
}

/// Writes `bytes` to `target` without ever leaving a half-written file.
pub fn write_atomic(target: &Path, bytes: &[u8]) -> Result<(), StorageError> {
    write_atomic_with_failure(target, bytes, |_| Ok(()))
}

/// Same as [`write_atomic`], calling `before` before each step. An error
/// returned by `before` simulates the failure of that step. For tests only.
pub fn write_atomic_with_failure(
    target: &Path,
    bytes: &[u8],
    mut before: impl FnMut(Step) -> io::Result<()>,
) -> Result<(), StorageError> {
    let tmp = temp(target)?;
    let mut created = false;
    let r = (|| -> io::Result<()> {
        before(Step::CreateTemp)?;
        let mut f = open_new(&tmp)?;
        created = true;
        before(Step::Write)?;
        f.write_all(bytes)?;
        before(Step::Sync)?;
        f.sync_all()?;
        drop(f);
        before(Step::Rename)?;
        fs::rename(&tmp, target)?;
        Ok(())
    })();
    if let Err(e) = r {
        // Only removes what this caller created (never somebody else's file).
        if created {
            let _ = fs::remove_file(&tmp);
        }
        return Err(e.into());
    }
    // After the rename the new file is already in place; syncing the
    // folder only makes the rename durable (does not apply on Windows). Some
    // file systems refuse a folder fsync: failure here does not undo
    // the write, so it is ignored (limitation in the security documentation).
    let _ = before(Step::SyncFolder);
    let _ = sync_folder(target);
    Ok(())
}

/// Creates the temp file only if it does not exist (`create_new`: does not follow a planted
/// symlink) and, on Unix, with permission 0600.
fn open_new(path: &Path) -> io::Result<File> {
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(path)
}

#[cfg(unix)]
fn sync_folder(target: &Path) -> io::Result<()> {
    match target.parent() {
        Some(p) if !p.as_os_str().is_empty() => File::open(p)?.sync_all(),
        _ => File::open(".")?.sync_all(),
    }
}

#[cfg(not(unix))]
fn sync_folder(_target: &Path) -> io::Result<()> {
    Ok(())
}

/// Reads a vault file, refusing anything over the size limit.
pub fn read_file(path: &Path) -> Result<Vec<u8>, StorageError> {
    let f = File::open(path)?;
    let mut v = Vec::new();
    f.take(MAX_FILE_LEN as u64 + 1).read_to_end(&mut v)?;
    if v.len() > MAX_FILE_LEN {
        return Err(StorageError::FileTooLarge);
    }
    Ok(v)
}

/// Copies the vault to `<target>.bak` (before migrations), also atomically.
/// Returns the path of the copy.
pub fn backup(path: &Path) -> Result<PathBuf, StorageError> {
    let bytes = read_file(path)?;
    let bak = with_suffix(path, ".bak");
    write_atomic(&bak, &bytes)?;
    Ok(bak)
}
