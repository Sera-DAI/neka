use core::fmt;

/// Erros de criptografia. Texto estático: nunca inclui chave, senha ou dado.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroCripto {
    /// Parâmetros do KDF fora dos limites aceitos.
    ParametrosForaDosLimites,
    /// O KDF falhou (por exemplo, sem memória).
    FalhaKdf,
    /// Etiqueta não confere: chave errada ou dado adulterado.
    FalhaAutenticacao,
    /// O sistema operacional não forneceu aleatoriedade.
    AleatoriedadeIndisponivel,
    /// Tamanho de entrada inválido.
    TamanhoInvalido,
    /// Pedido ao gerador de senhas impossível de atender.
    GeradorInvalido,
}

impl fmt::Display for ErroCripto {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::ParametrosForaDosLimites => "key derivation parameters out of bounds",
            Self::FalhaKdf => "key derivation failed",
            Self::FalhaAutenticacao => "incorrect password or tampered file",
            Self::AleatoriedadeIndisponivel => "randomness source unavailable",
            Self::TamanhoInvalido => "invalid data size",
            Self::GeradorInvalido => "invalid request to the password generator",
        })
    }
}

impl std::error::Error for ErroCripto {}
