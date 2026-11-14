//! Local device configuration: data folder, device id and
//! sync folder. Lives in `config.txt` (key=value), holds no secrets.

use std::fs;
use std::path::{Path, PathBuf};

use neka_core::api::{new_device_id, Uuid};

pub struct Config {
    pub dir: PathBuf,
    pub device_id: Uuid,
    pub sync_folder: Option<PathBuf>,
    pub last_sync: Option<i64>,
}

/// Default folder: `%APPDATA%\neka` on Windows; `$XDG_CONFIG_HOME/neka` or
/// `~/.config/neka` on other systems.
pub fn default_folder() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("neka"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|p| p.join("neka"))
    }
}

/// Renames a vault created before the English rename (`cofre.neka`) to
/// `vault.neka`, together with its `.bak` copy. Never overwrites an existing
/// `vault.neka`.
fn migrate_legacy_vault_name(dir: &Path) {
    for suffix in ["", ".bak"] {
        let old = dir.join(format!("cofre.neka{suffix}"));
        let new = dir.join(format!("vault.neka{suffix}"));
        if old.is_file() && !new.exists() {
            let _ = fs::rename(&old, &new);
        }
    }
}

impl Config {
    pub fn file(dir: &Path) -> PathBuf {
        dir.join("config.txt")
    }

    pub fn vault(&self) -> PathBuf {
        self.dir.join("vault.neka")
    }

    /// Reads the configuration, creating the folder and a device id the first time.
    pub fn load(dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&dir).map_err(|_| "could not create the data folder".to_owned())?;
        let mut c = Config {
            dir,
            device_id: Uuid::nil(),
            sync_folder: None,
            last_sync: None,
        };
        if let Ok(text) = fs::read_to_string(Self::file(&c.dir)) {
            for line in text.lines() {
                let Some((k, v)) = line.split_once('=') else {
                    continue;
                };
                let v = v.trim();
                match k.trim() {
                    "device_id" => c.device_id = Uuid::parse_str(v).unwrap_or(Uuid::nil()),
                    // Legacy (pre-English) key names are still read.
                    "sync_folder" | "pasta_sync" if !v.is_empty() => {
                        c.sync_folder = Some(PathBuf::from(v))
                    }
                    "last_sync" | "ultima_sync" => c.last_sync = v.parse().ok(),
                    _ => {}
                }
            }
        }
        migrate_legacy_vault_name(&c.dir);
        if c.device_id.is_nil() {
            c.device_id = new_device_id().map_err(|e| e.to_string())?;
            c.save()?;
        }
        Ok(c)
    }

    pub fn save(&self) -> Result<(), String> {
        let mut t = format!("device_id={}\n", self.device_id);
        if let Some(p) = &self.sync_folder {
            t.push_str(&format!("sync_folder={}\n", p.display()));
        }
        if let Some(u) = self.last_sync {
            t.push_str(&format!("last_sync={u}\n"));
        }
        let target = Self::file(&self.dir);
        let tmp = self.dir.join("config.txt.tmp");
        fs::write(&tmp, t)
            .and_then(|_| fs::rename(&tmp, &target))
            .map_err(|_| "could not save the configuration".to_owned())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp_dir(name: &str) -> PathBuf {
        let d = std::env::temp_dir().join(format!("neka-config-{name}-{}", std::process::id()));
        let _ = fs::remove_dir_all(&d);
        fs::create_dir_all(&d).unwrap();
        d
    }

    #[test]
    fn legacy_vault_file_and_config_keys_are_migrated() {
        let d = temp_dir("legacy");
        fs::write(d.join("cofre.neka"), b"vault bytes").unwrap();
        fs::write(d.join("cofre.neka.bak"), b"backup bytes").unwrap();
        fs::write(
            Config::file(&d),
            format!(
                "device_id={}\npasta_sync=/cloud\nultima_sync=42\n",
                Uuid::new_v4()
            ),
        )
        .unwrap();
        let c = Config::load(d.clone()).unwrap();
        assert_eq!(fs::read(c.vault()).unwrap(), b"vault bytes");
        assert_eq!(fs::read(d.join("vault.neka.bak")).unwrap(), b"backup bytes");
        assert!(!d.join("cofre.neka").exists());
        assert_eq!(c.sync_folder, Some(PathBuf::from("/cloud")));
        assert_eq!(c.last_sync, Some(42));
        c.save().unwrap();
        let text = fs::read_to_string(Config::file(&d)).unwrap();
        assert!(text.contains("sync_folder=/cloud") && text.contains("last_sync=42"));
    }

    #[test]
    fn legacy_migration_never_overwrites_an_existing_vault() {
        let d = temp_dir("keep");
        fs::write(d.join("cofre.neka"), b"old").unwrap();
        fs::write(d.join("vault.neka"), b"new").unwrap();
        let c = Config::load(d.clone()).unwrap();
        assert_eq!(fs::read(c.vault()).unwrap(), b"new");
        assert_eq!(fs::read(d.join("cofre.neka")).unwrap(), b"old");
    }
}
