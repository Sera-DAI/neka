//! Configuração local do dispositivo: pasta de dados, id do dispositivo e
//! pasta de sincronização. Fica em `config.txt` (chave=valor), sem segredos.

use std::fs;
use std::path::{Path, PathBuf};

use neka_core::api::{novo_id_dispositivo, Uuid};

pub struct Config {
    pub dir: PathBuf,
    pub device_id: Uuid,
    pub pasta_sync: Option<PathBuf>,
    pub ultima_sync: Option<i64>,
}

/// Pasta padrão: `%APPDATA%\neka` no Windows; `$XDG_CONFIG_HOME/neka` ou
/// `~/.config/neka` nos outros sistemas.
pub fn pasta_padrao() -> Option<PathBuf> {
    if cfg!(windows) {
        std::env::var_os("APPDATA").map(|p| PathBuf::from(p).join("neka"))
    } else {
        std::env::var_os("XDG_CONFIG_HOME")
            .map(PathBuf::from)
            .or_else(|| std::env::var_os("HOME").map(|h| PathBuf::from(h).join(".config")))
            .map(|p| p.join("neka"))
    }
}

impl Config {
    pub fn arquivo(dir: &Path) -> PathBuf {
        dir.join("config.txt")
    }

    pub fn cofre(&self) -> PathBuf {
        self.dir.join("cofre.neka")
    }

    /// Lê a configuração, criando a pasta e um id de dispositivo na primeira vez.
    pub fn carregar(dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&dir).map_err(|_| "could not create the data folder".to_owned())?;
        let mut c = Config {
            dir,
            device_id: Uuid::nil(),
            pasta_sync: None,
            ultima_sync: None,
        };
        if let Ok(texto) = fs::read_to_string(Self::arquivo(&c.dir)) {
            for linha in texto.lines() {
                let Some((k, v)) = linha.split_once('=') else {
                    continue;
                };
                let v = v.trim();
                match k.trim() {
                    "device_id" => c.device_id = Uuid::parse_str(v).unwrap_or(Uuid::nil()),
                    "pasta_sync" if !v.is_empty() => c.pasta_sync = Some(PathBuf::from(v)),
                    "ultima_sync" => c.ultima_sync = v.parse().ok(),
                    _ => {}
                }
            }
        }
        if c.device_id.is_nil() {
            c.device_id = novo_id_dispositivo().map_err(|e| e.to_string())?;
            c.salvar()?;
        }
        Ok(c)
    }

    pub fn salvar(&self) -> Result<(), String> {
        let mut t = format!("device_id={}\n", self.device_id);
        if let Some(p) = &self.pasta_sync {
            t.push_str(&format!("pasta_sync={}\n", p.display()));
        }
        if let Some(u) = self.ultima_sync {
            t.push_str(&format!("ultima_sync={u}\n"));
        }
        let destino = Self::arquivo(&self.dir);
        let tmp = self.dir.join("config.txt.tmp");
        fs::write(&tmp, t)
            .and_then(|_| fs::rename(&tmp, &destino))
            .map_err(|_| "could not save the configuration".to_owned())
    }
}
