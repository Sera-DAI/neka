//! Camada `io`: importação e exportação (CSV e KeePass KDBX 4).
//!
//! Só converte bytes de/para [`Registro`]. Quem decide o que entra no cofre
//! é a API. Nada daqui grava arquivo: a interface escolhe onde e avisa que
//! uma exportação em CSV fica **em claro**.

mod csv;
mod erro;
mod kdbx;

use std::collections::BTreeMap;
use std::fmt;

use zeroize::Zeroize;

pub use self::csv::{exportar_csv, importar_csv};
pub use erro::ErroIo;
pub use kdbx::{exportar_kdbx, importar_kdbx};

/// Uma conta a importar ou exportar, sem os metadados do cofre.
#[derive(Clone, Default, PartialEq, Eq)]
pub struct Registro {
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub extra: BTreeMap<String, String>,
}

impl fmt::Debug for Registro {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Registro { <oculto> }")
    }
}

impl Drop for Registro {
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
