use core::fmt;

/// Erros do domínio. Texto estático: nunca inclui conteúdo do cofre.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ErroModelo {
    TituloVazio,
    TituloLongo,
    UsuarioLongo,
    SenhaLonga,
    UrlLonga,
    NotasLongas,
    EtiquetasDemais,
    EtiquetaInvalida,
    ExtrasDemais,
    ExtraInvalido,
    HistoricoLongo,
    IdDuplicado,
    SchemaDesconhecido,
    CborInvalido,
    CborNaoSerializavel,
}

impl fmt::Display for ErroModelo {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(match self {
            Self::TituloVazio => "empty title",
            Self::TituloLongo => "title too long",
            Self::UsuarioLongo => "username too long",
            Self::SenhaLonga => "password too long",
            Self::UrlLonga => "address too long",
            Self::NotasLongas => "notes too long",
            Self::EtiquetasDemais => "too many tags",
            Self::EtiquetaInvalida => "empty or too long tag",
            Self::ExtrasDemais => "too many extra fields",
            Self::ExtraInvalido => "extra field with empty name or size exceeded",
            Self::HistoricoLongo => "password history too long",
            Self::IdDuplicado => "duplicate identifier in the vault",
            Self::SchemaDesconhecido => "unknown vault body schema version",
            Self::CborInvalido => "invalid vault body",
            Self::CborNaoSerializavel => "failed to serialize the vault body",
        })
    }
}

impl std::error::Error for ErroModelo {}
