//! Camada de aplicação: a única porta de entrada para as interfaces
//! (`neka-cli`, e `neka-android` no futuro). Operações de alto nível sobre
//! um cofre, com erros como códigos tipados e texto estático.
//!
//! Tempo (`agora`, ms desde 1970 UTC) e o identificador do dispositivo entram
//! por parâmetro: o relógio e o armazenamento do dispositivo são da interface.

mod cofre;
mod erro;
mod importar;
mod recuperacao;
mod sincronizar;

pub use crate::io::{
    exportar_csv as gerar_csv, exportar_kdbx as gerar_kdbx, importar_csv as ler_csv,
    importar_kdbx as ler_kdbx, ErroIo, Registro,
};
pub use cofre::{Cofre, Credencial, NovaEntrada};
pub use erro::ErroApi;
pub use importar::RelatorioImportacao;
pub use recuperacao::{
    de_texto as chave_de_recuperacao_de_texto, para_texto as chave_de_recuperacao_para_texto,
};
pub use sincronizar::{RelatorioSync, BACKUPS_MANTIDOS};

// Tipos que as interfaces precisam ver sem tocar nas camadas de baixo.
pub use crate::crypto::{gerar_senha, Chave, Classes, ParametrosKdf};
pub use crate::model::{Entrada, Uuid};

/// Versão do núcleo, para a interface mostrar em `--version`.
pub const VERSAO: &str = env!("CARGO_PKG_VERSION");

/// Identificador aleatório para um dispositivo novo (a interface guarda).
pub fn novo_id_dispositivo() -> Result<Uuid, ErroApi> {
    let mut b = [0u8; 16];
    crate::crypto::aleatorio(&mut b)?;
    Ok(uuid::Builder::from_random_bytes(b).into_uuid())
}
