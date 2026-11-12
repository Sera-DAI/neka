use core::fmt;

use crate::crypto::ErroCripto;
use crate::model::ErroModelo;

/// Erros de armazenamento. Texto estático; erros de E/S guardam só o tipo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroArmazenamento {
    /// Não começa com `NEKA` ou a estrutura está quebrada.
    FormatoInvalido,
    /// Arquivo de versão mais nova que este programa conhece.
    VersaoMaisNova,
    /// Versão antiga sem migração conhecida.
    VersaoNaoSuportada,
    /// `flags` com bits que este programa não conhece.
    FlagsDesconhecidas,
    /// Arquivo maior que o limite aceito.
    ArquivoGrandeDemais,
    /// Não há compartimento do tipo pedido.
    SemCompartimento,
    Cripto(ErroCripto),
    Modelo(ErroModelo),
    Io(std::io::ErrorKind),
}

impl fmt::Display for ErroArmazenamento {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::FormatoInvalido => f.write_str("file is not a valid neka vault"),
            Self::VersaoMaisNova => {
                f.write_str("vault created by a newer version of neka; update the program")
            }
            Self::VersaoNaoSuportada => f.write_str("unsupported vault version"),
            Self::FlagsDesconhecidas => f.write_str("vault uses unknown features"),
            Self::ArquivoGrandeDemais => f.write_str("file too large to be a vault"),
            Self::SemCompartimento => f.write_str("vault has no such unlock method"),
            Self::Cripto(e) => fmt::Display::fmt(e, f),
            Self::Modelo(e) => fmt::Display::fmt(e, f),
            Self::Io(_) => f.write_str("file read or write failure"),
        }
    }
}

impl std::error::Error for ErroArmazenamento {}

impl From<ErroCripto> for ErroArmazenamento {
    fn from(e: ErroCripto) -> Self {
        Self::Cripto(e)
    }
}

impl From<ErroModelo> for ErroArmazenamento {
    fn from(e: ErroModelo) -> Self {
        Self::Modelo(e)
    }
}

impl From<std::io::Error> for ErroArmazenamento {
    fn from(e: std::io::Error) -> Self {
        Self::Io(e.kind())
    }
}
