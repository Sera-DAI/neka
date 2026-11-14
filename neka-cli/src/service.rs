//! Connects the interactive screen to the real vault (`neka_core::api`). Keeps the vault
//! open only while the screen is unlocked: on lock, the vault and the file
//! lock are released.

use std::path::PathBuf;

use neka_core::api::{
    generate_csv, generate_kdbx, generate_password, read_csv, read_kdbx, recovery_key_to_text,
    ApiError, Credential, Entry, KdfParams, NewEntry, SyncReport, Uuid, Vault,
};
use zeroize::Zeroizing;

use crate::config::Config;
use crate::forms::{Data, DialogKind};
use crate::tui::{Effect, ListRow, Service};

const LOCKED: &str = "the vault is locked";

pub struct VaultService {
    cfg: Config,
    auto_sync: bool,
    kdf: KdfParams,
    vault: Option<Vault>,
    /// The master password is kept only while unlocked (sync needs it).
    password: Option<Zeroizing<String>>,
    pending: Option<String>,
}

impl VaultService {
    pub fn new(cfg: Config, auto_sync: bool) -> Self {
        VaultService {
            cfg,
            auto_sync,
            kdf: KdfParams::DEFAULT,
            vault: None,
            password: None,
            pending: None,
        }
    }

    fn open(&mut self) -> Result<&mut Vault, String> {
        self.vault.as_mut().ok_or_else(|| LOCKED.to_owned())
    }

    /// Syncs with the configured folder, printing nothing.
    fn sync_inner(&mut self) -> Result<SyncReport, String> {
        let folder = self
            .cfg
            .sync_folder
            .clone()
            .ok_or("no sync folder yet: press f to choose one")?;
        let password = self.password.clone().ok_or_else(|| LOCKED.to_owned())?;
        let vault = self.vault.as_mut().ok_or_else(|| LOCKED.to_owned())?;
        let r = match vault.sync(&folder, password.as_bytes(), None, crate::now()) {
            Err(ApiError::PasswordChangedOnOtherDevice) => {
                return Err(
                    "the master password was changed on another device; run `neka sync` to enter the new one"
                        .into(),
                )
            }
            Err(e) => return Err(crate::error(e)),
            Ok(r) => r,
        };
        self.cfg.last_sync = Some(crate::now());
        self.cfg.save()?;
        Ok(r)
    }

    /// Automatic sync after writing; a failure becomes a warning.
    fn after_write(&mut self) {
        if self.auto_sync && self.cfg.sync_folder.is_some() {
            if let Err(m) = self.sync_inner() {
                self.pending = Some(format!("sync failed: {m}"));
            }
        }
    }

    fn current_rows(&self) -> Vec<ListRow> {
        self.vault.as_ref().map(list_rows).unwrap_or_default()
    }
}

fn summary(r: &SyncReport) -> String {
    let mut t = format!(
        "Synced: {} file(s) read, {} conflict copy(ies) absorbed{}.",
        r.merged.len(),
        r.removed.len(),
        if r.changed_locally {
            ", local vault updated"
        } else {
            ", nothing new"
        }
    );
    if r.adopted_new_password {
        t.push_str(" This device now uses the newer master password.");
    }
    if !r.old_copies.is_empty() {
        t.push_str(&format!(
            " {} old-password copy(ies) were not merged.",
            r.old_copies.len()
        ));
    }
    if !r.not_open.is_empty() {
        t.push_str(&format!(
            " {} file(s) do not open with this password.",
            r.not_open.len()
        ));
    }
    t
}

/// Strips quotes and spaces from a pasted path.
fn path_of(text: &str) -> PathBuf {
    PathBuf::from(text.trim().trim_matches('"'))
}

fn list_rows(vault: &Vault) -> Vec<ListRow> {
    let mut es: Vec<&Entry> = vault.entries().iter().collect();
    es.sort_by_key(|e| e.title.to_lowercase());
    es.iter()
        .map(|e| ListRow {
            id: e.id.to_string(),
            title: e.title.clone(),
            username: e.username.clone(),
            url: e.url.clone(),
            notes: e.notes.clone(),
            password: zeroize::Zeroizing::new(e.password.clone()),
            password_date: crate::data(crate::password_date(e)),
            tags: e.tags.clone(),
        })
        .collect()
}

fn uuid(id: &str) -> Result<Uuid, String> {
    id.parse().map_err(|_| "invalid account id".to_owned())
}

impl Service for VaultService {
    fn unlock(&mut self, password: &str) -> Result<Vec<ListRow>, String> {
        // Releases the old lock before trying to open again.
        self.vault = None;
        self.password = None;
        let vault = Vault::open(
            &self.cfg.vault(),
            self.cfg.device_id,
            &Credential::Password(password.as_bytes()),
        )
        .map_err(crate::error)?;
        self.vault = Some(vault);
        self.password = Some(Zeroizing::new(password.to_owned()));
        self.after_write();
        Ok(self.current_rows())
    }

    fn save(&mut self, id: Option<&str>, d: Data) -> Result<(Vec<ListRow>, String), String> {
        let now = crate::now();
        let vault = self.open()?;
        let id = match id {
            None => vault
                .add(
                    NewEntry {
                        title: d.title.to_string(),
                        username: d.username.to_string(),
                        password: d.password.to_string(),
                        url: d.url.to_string(),
                        notes: d.notes.to_string(),
                        tags: d.tags.clone(),
                        ..Default::default()
                    },
                    now,
                )
                .map_err(crate::error)?,
            Some(id) => {
                let id = uuid(id)?;
                vault
                    .edit(
                        id,
                        |e| {
                            e.title = d.title.to_string();
                            e.username = d.username.to_string();
                            e.password = d.password.to_string();
                            e.url = d.url.to_string();
                            e.notes = d.notes.to_string();
                            e.tags = d.tags.clone();
                        },
                        now,
                    )
                    .map_err(crate::error)?;
                id
            }
        };
        vault.save().map_err(crate::error)?;
        self.after_write();
        Ok((self.current_rows(), id.to_string()))
    }

    fn remove(&mut self, id: &str) -> Result<Vec<ListRow>, String> {
        let id = uuid(id)?;
        let now = crate::now();
        let vault = self.open()?;
        vault.remove(id, now).map_err(crate::error)?;
        vault.save().map_err(crate::error)?;
        self.after_write();
        Ok(self.current_rows())
    }

    fn generate(&mut self, size: usize, symbols: bool) -> Result<String, String> {
        generate_password(size, crate::classes(!symbols)).map_err(|e| e.to_string())
    }

    fn lock(&mut self) {
        self.vault = None;
        self.password = None;
    }

    fn sync(&mut self) -> Result<Effect, String> {
        let r = self.sync_inner()?;
        Ok(Effect::List(self.current_rows(), summary(&r)))
    }

    fn pending_warning(&mut self) -> Option<String> {
        self.pending.take()
    }

    fn dialog(&mut self, kind: DialogKind, v: Vec<Zeroizing<String>>) -> Result<Effect, String> {
        match kind {
            DialogKind::Import => {
                let file = path_of(&v[0]);
                let bytes = Zeroizing::new(
                    std::fs::read(&file)
                        .map_err(|_| format!("could not read {}", file.display()))?,
                );
                let kdbx = bytes.starts_with(&[0x03, 0xd9, 0xa2, 0x9a]);
                let records = if kdbx {
                    if v[1].is_empty() {
                        return Err("enter the KeePass file password".into());
                    }
                    read_kdbx(&bytes, &v[1]).map_err(|e| e.to_string())?
                } else {
                    read_csv(&bytes).map_err(|e| e.to_string())?
                };
                let vault = self.open()?;
                let r = vault.import(&records, crate::now()).map_err(crate::error)?;
                vault.save().map_err(crate::error)?;
                self.after_write();
                let mut m = format!(
                    "Imported: {} · already existed: {} · rejected: {}.",
                    r.added, r.duplicates, r.invalid
                );
                if !kdbx {
                    m.push_str(" Delete the source CSV: it is plain text.");
                }
                Ok(Effect::List(self.current_rows(), m))
            }
            DialogKind::Export => {
                let file = path_of(&v[0]);
                if file.exists() {
                    return Err(format!("{} already exists", file.display()));
                }
                let ext = file
                    .extension()
                    .and_then(|e| e.to_str())
                    .map(str::to_lowercase)
                    .unwrap_or_default();
                let regs = self.open()?.records();
                let bytes = match ext.as_str() {
                    "kdbx" => {
                        if v[1].is_empty() {
                            return Err("choose a password for the KDBX file".into());
                        }
                        generate_kdbx(&regs, &v[1]).map_err(|e| e.to_string())?
                    }
                    "csv" => {
                        if !v[2].eq_ignore_ascii_case("plaintext") {
                            return Err(
                                "a CSV is unencrypted on disk: type PLAINTEXT to confirm, or use .kdbx"
                                    .into(),
                            );
                        }
                        generate_csv(&regs).map_err(|e| e.to_string())?
                    }
                    _ => return Err("use the .kdbx or .csv extension".into()),
                };
                let bytes = Zeroizing::new(bytes);
                std::fs::write(&file, bytes.as_slice())
                    .map_err(|_| format!("could not write {}", file.display()))?;
                Ok(Effect::Warning(format!(
                    "Exported {} entry(ies) to {}.",
                    regs.len(),
                    file.display()
                )))
            }
            DialogKind::SyncFolder => {
                if v[0].is_empty() {
                    self.cfg.sync_folder = None;
                    self.cfg.save()?;
                    return Ok(Effect::Warning("Syncing turned off.".into()));
                }
                let p = path_of(&v[0]);
                if !p.is_dir() {
                    return Err(format!("{} is not a folder", p.display()));
                }
                let p = p.canonicalize().unwrap_or(p);
                self.cfg.sync_folder = Some(p.clone());
                self.cfg.save()?;
                Ok(Effect::Warning(format!(
                    "Sync folder: {}. Press s to sync now.",
                    p.display()
                )))
            }
            DialogKind::MasterPassword => {
                let new = v[0].clone();
                let kdf = self.kdf;
                let vault = self.open()?;
                let rec = vault
                    .change_master_password(new.as_bytes(), kdf)
                    .map_err(crate::error)?;
                vault.save().map_err(crate::error)?;
                self.password = Some(new);
                self.after_write();
                Ok(Effect::Key(
                    Zeroizing::new(recovery_key_to_text(&rec)),
                    None,
                ))
            }
            DialogKind::Create => {
                let path = self.cfg.vault();
                if path.exists() {
                    return Err("a vault already exists on this device".into());
                }
                let new = v[0].clone();
                let (vault, rec) =
                    Vault::create(&path, self.cfg.device_id, new.as_bytes(), self.kdf)
                        .map_err(crate::error)?;
                self.vault = Some(vault);
                self.password = Some(new);
                Ok(Effect::Key(
                    Zeroizing::new(recovery_key_to_text(&rec)),
                    Some(self.current_rows()),
                ))
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use neka_core::api::KdfParams;

    const PASSWORD: &str = "password master fake";

    fn fast() -> KdfParams {
        KdfParams {
            mem_kib: KdfParams::MEM_MIN_KIB,
            iters: 1,
            lanes: 1,
        }
    }

    fn folder(name: &str) -> PathBuf {
        let dir = std::env::temp_dir().join(format!("neka-service-{name}-{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&dir);
        std::fs::create_dir_all(&dir).unwrap();
        dir
    }

    /// Service over a new device, with no vault yet.
    fn empty(dir: PathBuf) -> VaultService {
        let cfg = Config::load(dir).unwrap();
        let mut s = VaultService::new(cfg, true);
        s.kdf = fast();
        s
    }

    /// Service with a created vault (the real app does this through the Create dialog).
    fn new(name: &str) -> (VaultService, PathBuf) {
        let s = empty(folder(name));
        let path = s.cfg.vault();
        let (c, _) = Vault::create(&path, s.cfg.device_id, PASSWORD.as_bytes(), fast()).unwrap();
        drop(c);
        (s, path)
    }

    fn text(v: &[&str]) -> Vec<Zeroizing<String>> {
        v.iter().map(|t| Zeroizing::new((*t).to_owned())).collect()
    }

    fn data(title: &str, password: &str) -> Data {
        use zeroize::Zeroizing as Z;
        Data {
            title: Z::new(title.into()),
            username: Z::new("ana".into()),
            password: Z::new(password.into()),
            url: Z::new("https://x.invalid".into()),
            notes: Z::new("line1\nlinha2".into()),
            tags: vec!["a".into(), "b".into()],
        }
    }

    #[test]
    fn wrong_password_does_not_open_and_right_does() {
        let (mut s, _) = new("open");
        assert!(s.unlock("wrong").is_err());
        assert!(
            s.save(None, data("X", "p")).is_err(),
            "locked does not save"
        );
        assert!(s.unlock(PASSWORD).unwrap().is_empty());
    }

    #[test]
    fn add_edit_and_remove_write_to_the_file() {
        let (mut s, path) = new("crud");
        s.unlock(PASSWORD).unwrap();
        let (list, id) = s.save(None, data("Bank", "s1")).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].notes, "line1\nlinha2");
        assert_eq!(list[0].tags, ["a", "b"]);
        // Edits title and password.
        let (list, id2) = s.save(Some(&id), data("Bank New", "s2")).unwrap();
        assert_eq!(id, id2);
        assert_eq!(list[0].title, "Bank New");
        // Another service, reading from disk, sees the same and the password history.
        s.lock();
        let c = Vault::open(
            &path,
            Uuid::from_u128(7),
            &Credential::Password(PASSWORD.as_bytes()),
        )
        .unwrap();
        assert_eq!(c.entries().len(), 1);
        assert_eq!(c.entries()[0].password, "s2");
        assert_eq!(c.entries()[0].history.len(), 1, "old password in history");
        drop(c);
        s.unlock(PASSWORD).unwrap();
        assert!(s.remove(&id).unwrap().is_empty());
        s.lock();
        let c = Vault::open(
            &path,
            Uuid::from_u128(7),
            &Credential::Password(PASSWORD.as_bytes()),
        )
        .unwrap();
        assert!(c.entries().is_empty());
    }

    #[test]
    fn empty_title_is_refused_and_nothing_is_written() {
        let (mut s, _) = new("empty");
        s.unlock(PASSWORD).unwrap();
        assert!(s.save(None, data("", "p")).is_err());
        assert!(s.unlock(PASSWORD).unwrap().is_empty());
    }

    #[test]
    fn locking_releases_the_file_lock() {
        let (mut s, path) = new("lock");
        let open = || {
            Vault::open(
                &path,
                Uuid::from_u128(9),
                &Credential::Password(PASSWORD.as_bytes()),
            )
        };
        s.unlock(PASSWORD).unwrap();
        assert!(open().is_err(), "unlocked: another process cannot open");
        s.lock();
        assert!(open().is_ok(), "locked: a lock foi solta");
    }

    #[test]
    fn generate_respects_length_and_symbols() {
        let (mut s, _) = new("generate");
        let p = s.generate(24, false).unwrap();
        assert_eq!(p.chars().count(), 24);
        assert!(p.chars().all(|c| c.is_ascii_alphanumeric()));
        assert!(s.generate(2, true).is_err());
    }

    use crate::forms::DialogKind as T;

    #[test]
    fn creating_the_vault_via_the_dialog_opens_and_shows_the_key() {
        let mut s = empty(folder("create"));
        match s.dialog(T::Create, text(&["password-long-1", "password-long-1"])) {
            Ok(Effect::Key(key, Some(list))) => {
                assert!(key.len() > 20 && list.is_empty());
            }
            _ => panic!("esperava a key e a list empty"),
        }
        assert!(s.cfg.vault().exists());
        // Already exists: does not overwrite.
        assert!(s
            .dialog(T::Create, text(&["other-password-1", "other-password-1"]))
            .is_err());
        // Unlocks with the created password.
        s.lock();
        assert!(s.unlock("password-long-1").unwrap().is_empty());
    }

    #[test]
    fn changing_the_master_password_rotates_the_key_and_only_the_new_opens() {
        let (mut s, _) = new("password");
        s.unlock(PASSWORD).unwrap();
        let r = s.dialog(
            T::MasterPassword,
            text(&["new-password-123", "new-password-123"]),
        );
        assert!(matches!(r, Ok(Effect::Key(ref c, None)) if c.len() > 20));
        s.lock();
        assert!(
            s.unlock(PASSWORD).is_err(),
            "the old password no longer opens"
        );
        assert!(s.unlock("new-password-123").is_ok());
    }

    #[test]
    fn sync_folder_turns_on_off_and_refuses_what_is_not_a_folder() {
        let (mut s, _) = new("folder");
        let cloud = folder("folder-cloud");
        let file = cloud.join("file.txt");
        std::fs::write(&file, "x").unwrap();
        assert!(s
            .dialog(T::SyncFolder, text(&[file.to_str().unwrap()]))
            .is_err());
        let quotes = format!("\"{}\"", cloud.display());
        assert!(s.dialog(T::SyncFolder, text(&[&quotes])).is_ok());
        assert!(s.cfg.sync_folder.is_some());
        assert!(s.dialog(T::SyncFolder, text(&[""])).is_ok());
        assert!(s.cfg.sync_folder.is_none());
        assert!(s.sync().is_err(), "without folder, sync explica");
    }

    #[test]
    fn two_devices_exchange_accounts_through_the_folder() {
        let (mut a, _) = new("sync-a");
        let cloud = folder("sync-cloud");
        a.dialog(T::SyncFolder, text(&[cloud.to_str().unwrap()]))
            .unwrap();
        a.unlock(PASSWORD).unwrap();
        a.save(None, data("Bank", "s1")).unwrap();
        assert!(a.pending_warning().is_none());
        let files = std::fs::read_dir(&cloud).unwrap().count();
        assert!(files >= 1, "save publicou na folder");
        // Device B brings the vault from the cloud and syncs.
        let dir_b = folder("sync-b");
        let source = std::fs::read_dir(&cloud)
            .unwrap()
            .map(|e| e.unwrap().path())
            .find(|p| p.extension().is_some_and(|e| e == "neka"))
            .unwrap();
        let mut b = empty(dir_b);
        Vault::clone(
            &source,
            &b.cfg.vault(),
            b.cfg.device_id,
            PASSWORD.as_bytes(),
        )
        .unwrap();
        b.cfg.sync_folder = Some(cloud.clone());
        let list = b.unlock(PASSWORD).unwrap();
        assert_eq!(list.len(), 1);
        assert_eq!(list[0].title, "Bank");
        // B adds; A syncs manually and sees it.
        b.save(None, data("Email", "s2")).unwrap();
        match a.sync() {
            Ok(Effect::List(l, m)) => {
                assert_eq!(l.len(), 2);
                assert!(m.starts_with("Synced"));
            }
            _ => panic!("esperava a list sincronizada"),
        }
    }

    #[test]
    fn automatic_sync_failure_becomes_a_warning_and_keeps_the_save() {
        let (mut s, _) = new("sync-failure");
        let cloud = folder("sync-failure-cloud");
        s.dialog(T::SyncFolder, text(&[cloud.to_str().unwrap()]))
            .unwrap();
        s.unlock(PASSWORD).unwrap();
        std::fs::remove_dir_all(&cloud).unwrap();
        let (list, _) = s.save(None, data("Bank", "s1")).unwrap();
        assert_eq!(list.len(), 1);
        let warning = s.pending_warning().expect("avisou");
        assert!(warning.starts_with("sync failed"), "{warning}");
        assert!(s.pending_warning().is_none(), "the warning shows only once");
    }

    #[test]
    fn export_csv_and_import_into_another_vault() {
        let (mut a, _) = new("exp-a");
        a.unlock(PASSWORD).unwrap();
        a.save(None, data("Bank", "s1")).unwrap();
        let output = folder("exp-output").join("contas.csv");
        let p = output.to_str().unwrap();
        // CSV requires the spelled-out confirmation.
        assert!(a.dialog(T::Export, text(&[p, "", ""])).is_err());
        assert!(!output.exists());
        a.dialog(T::Export, text(&[p, "", "plaintext"])).unwrap();
        assert!(
            a.dialog(T::Export, text(&[p, "", "plaintext"])).is_err(),
            "does not overwrite"
        );
        assert!(a.dialog(T::Export, text(&["x.txt", "", ""])).is_err());
        let (mut b, _) = new("exp-b");
        b.unlock(PASSWORD).unwrap();
        match b.dialog(T::Import, text(&[p, ""])) {
            Ok(Effect::List(l, m)) => {
                assert_eq!(l.len(), 1);
                assert!(m.contains("Imported: 1") && m.contains("plain text"));
            }
            _ => panic!("esperava a list importada"),
        }
        // Importing again does not duplicate.
        match b.dialog(T::Import, text(&[p, ""])) {
            Ok(Effect::List(l, m)) => assert!(l.len() == 1 && m.contains("already existed: 1")),
            _ => panic!(),
        }
        assert!(b.dialog(T::Import, text(&["not-exists.csv", ""])).is_err());
    }

    #[test]
    fn export_kdbx_requires_a_password_and_imports_back() {
        let (mut a, _) = new("kdbx-a");
        a.unlock(PASSWORD).unwrap();
        a.save(None, data("Bank", "s1")).unwrap();
        let output = folder("kdbx-output").join("contas.kdbx");
        let p = output.to_str().unwrap();
        assert!(a.dialog(T::Export, text(&[p, "", ""])).is_err());
        a.dialog(T::Export, text(&[p, "password-do-file", ""]))
            .unwrap();
        let (mut b, _) = new("kdbx-b");
        b.unlock(PASSWORD).unwrap();
        assert!(
            b.dialog(T::Import, text(&[p, ""])).is_err(),
            "kdbx without password"
        );
        assert!(b.dialog(T::Import, text(&[p, "wrong"])).is_err());
        assert!(b.dialog(T::Import, text(&[p, "password-do-file"])).is_ok());
    }

    #[test]
    fn actions_on_a_locked_vault_are_refused() {
        let (mut s, _) = new("locked");
        assert!(s
            .dialog(
                T::MasterPassword,
                text(&["new-password-123", "new-password-123"])
            )
            .is_err());
        assert!(s.dialog(T::Import, text(&["x.csv", ""])).is_err());
        assert!(s.sync().is_err());
    }
}
