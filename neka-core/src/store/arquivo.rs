//! Codificação binária do `.neka` v1. Inteiros little-endian.

use uuid::Uuid;

use super::ErroArmazenamento;
use crate::crypto::{
    ChaveEmbrulhada, Nonce, ParametrosKdf, Sal, TipoSlot, TAM_EMBRULHADA, TAM_ETIQUETA, TAM_NONCE,
    TAM_SAL,
};

pub const MAGIC: [u8; 4] = *b"NEKA";
pub const VERSAO_ATUAL: u16 = 1;
/// Limite de tamanho aceito na leitura (64 MiB).
pub const TAM_MAX_ARQUIVO: usize = 64 * 1024 * 1024;

const KDF_ARGON2ID: u8 = 1;
const MAX_SLOTS: usize = 8;
/// magic + version + flags + vault_id + key_epoch: o que o AAD do corpo cobre (D14, D15).
pub(crate) const TAM_PREFIXO: usize = 4 + 2 + 2 + 16 + 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub tipo: TipoSlot,
    pub chave: ChaveEmbrulhada,
}

/// Arquivo `.neka` decodificado, ainda cifrado.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Arquivo {
    pub vault_id: Uuid,
    /// Geração da chave de dados; sobe a cada troca de senha mestra (D15).
    pub key_epoch: u32,
    pub kdf: ParametrosKdf,
    pub sal: Sal,
    pub slots: Vec<Slot>,
    pub body_nonce: Nonce,
    /// Texto cifrado do corpo, com a etiqueta.
    pub body: Vec<u8>,
}

impl Arquivo {
    fn prefixo(vault_id: &Uuid, epoca: u32) -> [u8; TAM_PREFIXO] {
        let mut p = [0u8; TAM_PREFIXO];
        p[..4].copy_from_slice(&MAGIC);
        p[4..6].copy_from_slice(&VERSAO_ATUAL.to_le_bytes());
        p[6..8].copy_from_slice(&0u16.to_le_bytes());
        p[8..24].copy_from_slice(vault_id.as_bytes());
        p[24..28].copy_from_slice(&epoca.to_le_bytes());
        p
    }

    /// AAD do corpo (D14): prefixo fixo, `body_nonce` e `body_len`.
    pub(crate) fn aad_corpo(vault_id: &Uuid, epoca: u32, nonce: &Nonce, body_len: u64) -> Vec<u8> {
        let mut a = Vec::with_capacity(TAM_PREFIXO + TAM_NONCE + 8);
        a.extend_from_slice(&Self::prefixo(vault_id, epoca));
        a.extend_from_slice(&nonce.0);
        a.extend_from_slice(&body_len.to_le_bytes());
        a
    }

    pub fn codificar(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(128 + self.slots.len() * 73 + self.body.len());
        o.extend_from_slice(&Self::prefixo(&self.vault_id, self.key_epoch));
        o.push(KDF_ARGON2ID);
        o.extend_from_slice(&self.kdf.mem_kib.to_le_bytes());
        o.extend_from_slice(&self.kdf.iters.to_le_bytes());
        o.extend_from_slice(&self.kdf.lanes.to_le_bytes());
        o.extend_from_slice(&self.sal.0);
        o.push(self.slots.len() as u8);
        for s in &self.slots {
            o.push(s.tipo as u8);
            o.extend_from_slice(&s.chave.nonce.0);
            o.extend_from_slice(&s.chave.cifrada);
        }
        o.extend_from_slice(&self.body_nonce.0);
        o.extend_from_slice(&(self.body.len() as u64).to_le_bytes());
        o.extend_from_slice(&self.body);
        o
    }

    /// Decodifica e valida a estrutura. Não decifra nada.
    pub fn decodificar(b: &[u8]) -> Result<Self, ErroArmazenamento> {
        use ErroArmazenamento::*;
        if b.len() > TAM_MAX_ARQUIVO {
            return Err(ArquivoGrandeDemais);
        }
        let mut l = Leitor { b, i: 0 };
        if l.ler::<4>()? != MAGIC {
            return Err(FormatoInvalido);
        }
        let versao = u16::from_le_bytes(l.ler()?);
        if versao > VERSAO_ATUAL {
            return Err(VersaoMaisNova);
        }
        if versao < VERSAO_ATUAL {
            return Err(VersaoNaoSuportada);
        }
        if u16::from_le_bytes(l.ler()?) != 0 {
            return Err(FlagsDesconhecidas);
        }
        let vault_id = Uuid::from_bytes(l.ler()?);
        let key_epoch = u32::from_le_bytes(l.ler()?);
        if l.ler::<1>()?[0] != KDF_ARGON2ID {
            return Err(FormatoInvalido);
        }
        let kdf = ParametrosKdf {
            mem_kib: u32::from_le_bytes(l.ler()?),
            iters: u32::from_le_bytes(l.ler()?),
            lanes: u32::from_le_bytes(l.ler()?),
        };
        kdf.validar()?;
        let sal = Sal(l.ler::<TAM_SAL>()?);
        let n = l.ler::<1>()?[0] as usize;
        if n == 0 || n > MAX_SLOTS {
            return Err(FormatoInvalido);
        }
        let mut slots: Vec<Slot> = Vec::with_capacity(n);
        for _ in 0..n {
            let tipo = TipoSlot::de_byte(l.ler::<1>()?[0]).ok_or(FormatoInvalido)?;
            if slots.iter().any(|s| s.tipo == tipo) {
                return Err(FormatoInvalido);
            }
            let nonce = Nonce(l.ler()?);
            let cifrada = l.ler::<TAM_EMBRULHADA>()?;
            slots.push(Slot {
                tipo,
                chave: ChaveEmbrulhada { nonce, cifrada },
            });
        }
        let body_nonce = Nonce(l.ler()?);
        let body_len = u64::from_le_bytes(l.ler()?);
        let resto = b.len() - l.i;
        if body_len != resto as u64 || resto < TAM_ETIQUETA {
            return Err(FormatoInvalido);
        }
        Ok(Self {
            vault_id,
            key_epoch,
            kdf,
            sal,
            slots,
            body_nonce,
            body: b[l.i..].to_vec(),
        })
    }
}

struct Leitor<'a> {
    b: &'a [u8],
    i: usize,
}

impl Leitor<'_> {
    fn ler<const N: usize>(&mut self) -> Result<[u8; N], ErroArmazenamento> {
        let fim = self
            .i
            .checked_add(N)
            .ok_or(ErroArmazenamento::FormatoInvalido)?;
        let s = self
            .b
            .get(self.i..fim)
            .ok_or(ErroArmazenamento::FormatoInvalido)?;
        self.i = fim;
        Ok(s.try_into().expect("tamanho conferido"))
    }
}
