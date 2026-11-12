use core::fmt;

use crate::crypto::ErroCripto;
use crate::model::ErroModelo;
use crate::store::ErroArmazenamento;

/// Erro da API. Cada variante tem um código estável (`codigo`) e um texto
/// estático (`Display`); nenhum carrega senha, chave ou conteúdo do cofre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroApi {
    /// Já existe um arquivo nesse caminho; o `neka` não sobrescreve cofres.
    ArquivoJaExiste,
    EntradaNaoEncontrada,
    /// Outro processo (ou outra instância) está com este cofre aberto.
    CofreEmUso,
    /// Texto da chave de recuperação mal formado ou com erro de digitação.
    ChaveRecuperacaoInvalida,
    /// Senha mestra vazia.
    SenhaVazia,
    /// A pasta de sincronização não existe ou é a própria pasta do cofre.
    PastaDeSyncInvalida,
    /// Um arquivo da pasta tem senha mais nova (trocada em outro
    /// dispositivo): é preciso digitar a senha nova.
    SenhaTrocadaEmOutroDispositivo,
    Armazenamento(ErroArmazenamento),
    Cripto(ErroCripto),
    Modelo(ErroModelo),
}

impl ErroApi {
    /// Código estável para interfaces e logs (nunca muda de significado).
    pub fn codigo(&self) -> &'static str {
        match self {
            Self::ArquivoJaExiste => "file_already_exists",
            Self::EntradaNaoEncontrada => "entry_not_found",
            Self::CofreEmUso => "vault_in_use",
            Self::ChaveRecuperacaoInvalida => "invalid_recovery_key",
            Self::SenhaVazia => "empty_password",
            Self::PastaDeSyncInvalida => "invalid_sync_folder",
            Self::SenhaTrocadaEmOutroDispositivo => "password_changed_on_another_device",
            Self::Armazenamento(ErroArmazenamento::Cripto(ErroCripto::FalhaAutenticacao))
            | Self::Cripto(ErroCripto::FalhaAutenticacao) => {
                "incorrect_credential_or_tampered_file"
            }
            Self::Armazenamento(ErroArmazenamento::VersaoMaisNova) => "newer_version",
            Self::Armazenamento(ErroArmazenamento::Io(_)) => "file_failure",
            Self::Armazenamento(_) => "invalid_vault",
            Self::Cripto(_) => "cryptographic_failure",
            Self::Modelo(_) => "invalid_data",
        }
    }
}

impl fmt::Display for ErroApi {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::ArquivoJaExiste => f.write_str("a file already exists at that path"),
            Self::EntradaNaoEncontrada => f.write_str("entry not found"),
            Self::CofreEmUso => f.write_str("the vault is open in another process"),
            Self::ChaveRecuperacaoInvalida => f.write_str("invalid recovery key"),
            Self::SenhaVazia => f.write_str("the master password cannot be empty"),
            Self::PastaDeSyncInvalida => {
                f.write_str("sync folder does not exist or is the same as the vault folder")
            }
            Self::SenhaTrocadaEmOutroDispositivo => {
                f.write_str("the master password was changed on another device")
            }
            Self::Armazenamento(e) => fmt::Display::fmt(e, f),
            Self::Cripto(e) => fmt::Display::fmt(e, f),
            Self::Modelo(e) => fmt::Display::fmt(e, f),
        }
    }
}

impl std::error::Error for ErroApi {}

impl From<ErroArmazenamento> for ErroApi {
    fn from(e: ErroArmazenamento) -> Self {
        Self::Armazenamento(e)
    }
}
impl From<ErroCripto> for ErroApi {
    fn from(e: ErroCripto) -> Self {
        Self::Cripto(e)
    }
}
impl From<ErroModelo> for ErroApi {
    fn from(e: ErroModelo) -> Self {
        Self::Modelo(e)
    }
}
