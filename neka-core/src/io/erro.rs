use core::fmt;

/// Erros de importação e exportação. Texto estático, sem conteúdo.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroIo {
    /// CSV mal formado ou sem cabeçalho reconhecível.
    CsvInvalido,
    /// O CSV não tem coluna de título nem de endereço.
    CsvSemColunas,
    /// Senha errada ou arquivo KDBX corrompido.
    KdbxNaoAbriu,
    /// Versão de KeePass que não sabemos ler.
    KdbxNaoSuportado,
    /// Falha ao gerar o KDBX.
    KdbxNaoGravou,
}

impl fmt::Display for ErroIo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::CsvInvalido => "invalid CSV file",
            Self::CsvSemColunas => "the CSV has no title, username, password or address columns",
            Self::KdbxNaoAbriu => "wrong password or corrupted KeePass file",
            Self::KdbxNaoSuportado => "unsupported KeePass file version",
            Self::KdbxNaoGravou => "failed to generate the KeePass file",
        })
    }
}

impl std::error::Error for ErroIo {}
