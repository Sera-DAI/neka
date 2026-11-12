use std::collections::BTreeMap;
use std::fmt;

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use zeroize::Zeroize;

use super::ErroModelo;

/// Máximo de senhas antigas guardadas por entrada (a especificação do formato).
pub const MAX_HISTORICO: usize = 10;

const MAX_TITULO: usize = 256;
const MAX_USUARIO: usize = 256;
const MAX_SENHA: usize = 4096;
const MAX_URL: usize = 2048;
const MAX_NOTAS: usize = 64 * 1024;
const MAX_ETIQUETAS: usize = 64;
const MAX_ETIQUETA: usize = 64;
const MAX_EXTRAS: usize = 64;
const MAX_EXTRA_NOME: usize = 64;
const MAX_EXTRA_VALOR: usize = 4096;

/// Senha anterior e o instante (ms desde 1970, UTC) em que foi trocada.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SenhaAntiga {
    pub password: String,
    pub changed_at: i64,
}

impl fmt::Debug for SenhaAntiga {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("SenhaAntiga")
            .field("password", &"<oculto>")
            .field("changed_at", &self.changed_at)
            .finish()
    }
}

impl Drop for SenhaAntiga {
    fn drop(&mut self) {
        self.password.zeroize();
    }
}

/// Entrada do cofre. Nomes dos campos seguem a especificação do formato.
#[derive(Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Entrada {
    pub id: Uuid,
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub extra: BTreeMap<String, String>,
    pub history: Vec<SenhaAntiga>,
    pub created_at: i64,
    pub updated_at: i64,
    pub clock: u64,
    pub device_id: Uuid,
}

/// O `Debug` oculta título, senha, notas e campos extras.
impl fmt::Debug for Entrada {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.debug_struct("Entrada")
            .field("id", &self.id)
            .field("clock", &self.clock)
            .field("device_id", &self.device_id)
            .field("segredos", &"<ocultos>")
            .finish()
    }
}

impl Drop for Entrada {
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

impl Entrada {
    /// Cria uma entrada nova. `now` em ms desde 1970 (UTC).
    pub fn nova(id: Uuid, device_id: Uuid, title: String, now: i64, clock: u64) -> Self {
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

    /// Troca a senha, guardando a anterior no histórico (no máximo
    /// [`MAX_HISTORICO`]; a mais antiga sai primeiro).
    pub fn trocar_senha(&mut self, nova: String, now: i64) {
        let antiga = std::mem::replace(&mut self.password, nova);
        if !antiga.is_empty() {
            self.history.push(SenhaAntiga {
                password: antiga,
                changed_at: now,
            });
            while self.history.len() > MAX_HISTORICO {
                self.history.remove(0);
            }
        }
        self.updated_at = now;
    }

    /// Confere os limites de tamanho e o formato dos campos.
    pub fn validar(&self) -> Result<(), ErroModelo> {
        if self.title.trim().is_empty() {
            return Err(ErroModelo::TituloVazio);
        }
        if self.title.len() > MAX_TITULO {
            return Err(ErroModelo::TituloLongo);
        }
        if self.username.len() > MAX_USUARIO {
            return Err(ErroModelo::UsuarioLongo);
        }
        if self.password.len() > MAX_SENHA {
            return Err(ErroModelo::SenhaLonga);
        }
        if self.url.len() > MAX_URL {
            return Err(ErroModelo::UrlLonga);
        }
        if self.notes.len() > MAX_NOTAS {
            return Err(ErroModelo::NotasLongas);
        }
        if self.tags.len() > MAX_ETIQUETAS {
            return Err(ErroModelo::EtiquetasDemais);
        }
        if self
            .tags
            .iter()
            .any(|t| t.is_empty() || t.len() > MAX_ETIQUETA)
        {
            return Err(ErroModelo::EtiquetaInvalida);
        }
        if self.extra.len() > MAX_EXTRAS {
            return Err(ErroModelo::ExtrasDemais);
        }
        if self
            .extra
            .iter()
            .any(|(k, v)| k.is_empty() || k.len() > MAX_EXTRA_NOME || v.len() > MAX_EXTRA_VALOR)
        {
            return Err(ErroModelo::ExtraInvalido);
        }
        if self.history.len() > MAX_HISTORICO
            || self.history.iter().any(|h| h.password.len() > MAX_SENHA)
        {
            return Err(ErroModelo::HistoricoLongo);
        }
        Ok(())
    }
}

/// Marca de exclusão. Não guarda nenhum campo da entrada original.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Lapide {
    pub id: Uuid,
    pub deleted_at: i64,
    pub clock: u64,
    pub device_id: Uuid,
}
