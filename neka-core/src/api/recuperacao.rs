//! Texto da chave de recuperação: base32 (RFC 4648, sem preenchimento) de
//! 32 bytes da chave mais 2 bytes de verificação, em grupos de 5 separados
//! por hífen. A verificação só pega erro de digitação; não é segurança.

use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use super::ErroApi;
use crate::crypto::{Chave, TAM_CHAVE};

const ALFABETO: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const TAM_VERIF: usize = 2;
const TAM_TOTAL: usize = TAM_CHAVE + TAM_VERIF;

fn verificacao(chave: &[u8; TAM_CHAVE]) -> [u8; TAM_VERIF] {
    let mut h = Sha256::new();
    h.update(b"neka-v1-recovery-check");
    h.update(chave);
    let d = h.finalize();
    [d[0], d[1]]
}

/// Texto para mostrar ao usuário uma única vez.
pub fn para_texto(chave: &Chave) -> String {
    let mut dados = [0u8; TAM_TOTAL];
    dados[..TAM_CHAVE].copy_from_slice(chave.como_bytes());
    dados[TAM_CHAVE..].copy_from_slice(&verificacao(chave.como_bytes()));
    let mut bits: u32 = 0;
    let mut n = 0;
    let mut saida = String::new();
    let mut cont = 0;
    for &b in dados.iter() {
        bits = (bits << 8) | b as u32;
        n += 8;
        while n >= 5 {
            n -= 5;
            if cont > 0 && cont % 5 == 0 {
                saida.push('-');
            }
            saida.push(ALFABETO[((bits >> n) & 31) as usize] as char);
            cont += 1;
        }
    }
    if n > 0 {
        if cont > 0 && cont % 5 == 0 {
            saida.push('-');
        }
        saida.push(ALFABETO[((bits << (5 - n)) & 31) as usize] as char);
    }
    dados.zeroize();
    saida
}

/// Lê o texto digitado: ignora hífens, espaços e maiúsculas/minúsculas.
pub fn de_texto(texto: &str) -> Result<Chave, ErroApi> {
    let mut bits: u32 = 0;
    let mut n = 0;
    let mut dados = Vec::with_capacity(TAM_TOTAL + 1);
    for c in texto.chars().filter(|c| !c.is_whitespace() && *c != '-') {
        let c = c.to_ascii_uppercase();
        let v = ALFABETO
            .iter()
            .position(|&a| a as char == c)
            .ok_or(ErroApi::ChaveRecuperacaoInvalida);
        let v = match v {
            Ok(v) => v as u32,
            Err(e) => {
                dados.zeroize();
                return Err(e);
            }
        };
        bits = (bits << 5) | v;
        n += 5;
        if n >= 8 {
            n -= 8;
            dados.push((bits >> n) as u8);
            if dados.len() > TAM_TOTAL {
                dados.zeroize();
                return Err(ErroApi::ChaveRecuperacaoInvalida);
            }
        }
    }
    // sobra de bits (menos de 8) deve ser zero, e o tamanho exato
    let ok = dados.len() == TAM_TOTAL && (bits & ((1 << n) - 1)) == 0;
    let r = if ok {
        let mut k = [0u8; TAM_CHAVE];
        k.copy_from_slice(&dados[..TAM_CHAVE]);
        let r = if dados[TAM_CHAVE..] == verificacao(&k) {
            Ok(Chave::de_bytes(k))
        } else {
            Err(ErroApi::ChaveRecuperacaoInvalida)
        };
        k.zeroize();
        r
    } else {
        Err(ErroApi::ChaveRecuperacaoInvalida)
    };
    dados.zeroize();
    r
}
