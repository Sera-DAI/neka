use uuid::Uuid;

use super::{abrir, selar, Chave, ErroCripto, Nonce, TAM_CHAVE, TAM_ETIQUETA};

/// Chave de dados cifrada (32) mais etiqueta (16).
pub const TAM_EMBRULHADA: usize = TAM_CHAVE + TAM_ETIQUETA;

const PREFIXO_AAD: &[u8] = b"neka-v1-keyslot";

/// Tipo de compartimento de chave (a especificação do formato).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum TipoSlot {
    Senha = 1,
    Recuperacao = 2,
}

impl TipoSlot {
    pub fn de_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(Self::Senha),
            2 => Some(Self::Recuperacao),
            _ => None,
        }
    }
}

/// Resultado do embrulho: o que vai gravado no compartimento.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ChaveEmbrulhada {
    pub nonce: Nonce,
    pub cifrada: [u8; TAM_EMBRULHADA],
}

fn aad(vault_id: &Uuid, epoca: u32, tipo: TipoSlot) -> Vec<u8> {
    let mut a = Vec::with_capacity(PREFIXO_AAD.len() + 21);
    a.extend_from_slice(PREFIXO_AAD);
    a.extend_from_slice(vault_id.as_bytes());
    a.extend_from_slice(&epoca.to_le_bytes());
    a.push(tipo as u8);
    a
}

/// Embrulha a chave de dados com a chave de embrulho, com nonce novo.
/// AAD: `"neka-v1-keyslot" || vault_id || key_epoch || slot_type` (D15).
pub fn embrulhar(
    embrulho: &Chave,
    dados: &Chave,
    vault_id: &Uuid,
    epoca: u32,
    tipo: TipoSlot,
) -> Result<ChaveEmbrulhada, ErroCripto> {
    let nonce = Nonce::aleatorio()?;
    let c = selar(
        embrulho,
        &nonce,
        &aad(vault_id, epoca, tipo),
        dados.como_bytes(),
    )?;
    let cifrada = c.try_into().map_err(|_| ErroCripto::TamanhoInvalido)?;
    Ok(ChaveEmbrulhada { nonce, cifrada })
}

/// Recupera a chave de dados. Chave errada, cofre, época ou tipo trocados, ou
/// qualquer byte alterado: [`ErroCripto::FalhaAutenticacao`].
pub fn desembrulhar(
    embrulho: &Chave,
    e: &ChaveEmbrulhada,
    vault_id: &Uuid,
    epoca: u32,
    tipo: TipoSlot,
) -> Result<Chave, ErroCripto> {
    let mut claro = abrir(embrulho, &e.nonce, &aad(vault_id, epoca, tipo), &e.cifrada)?;
    let r = <[u8; TAM_CHAVE]>::try_from(claro.as_slice()).map_err(|_| ErroCripto::TamanhoInvalido);
    zeroize::Zeroize::zeroize(&mut claro);
    r.map(Chave::de_bytes)
}
