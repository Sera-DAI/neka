use zeroize::Zeroize;

use super::{aleatorio, ErroCripto};

pub const TAM_SENHA_MIN: usize = 8;
pub const TAM_SENHA_MAX: usize = 256;

const MINUSCULAS: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const MAIUSCULAS: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITOS: &[u8] = b"0123456789";
const SIMBOLOS: &[u8] = b"!#$%&()*+,-./:;<=>?@[]^_{|}~";

/// Classes de caracteres pedidas. Cada classe marcada aparece ao menos uma vez.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classes {
    pub minusculas: bool,
    pub maiusculas: bool,
    pub digitos: bool,
    pub simbolos: bool,
}

impl Classes {
    pub const TODAS: Self = Self {
        minusculas: true,
        maiusculas: true,
        digitos: true,
        simbolos: true,
    };

    fn conjuntos(&self) -> Vec<&'static [u8]> {
        [
            (self.minusculas, MINUSCULAS),
            (self.maiusculas, MAIUSCULAS),
            (self.digitos, DIGITOS),
            (self.simbolos, SIMBOLOS),
        ]
        .into_iter()
        .filter_map(|(on, c)| on.then_some(c))
        .collect()
    }
}

/// Inteiro uniforme em `0..n` por rejeição (sem viés de módulo).
fn uniforme(n: usize) -> Result<usize, ErroCripto> {
    debug_assert!(n > 0 && n <= 256);
    let limite = 256 - (256 % n);
    loop {
        let mut b = [0u8; 1];
        aleatorio(&mut b)?;
        if (b[0] as usize) < limite {
            return Ok(b[0] as usize % n);
        }
    }
}

/// Gera uma senha aleatória com o sistema operacional como fonte.
/// Sorteia até conter todas as classes pedidas (rejeição, sem viés).
pub fn gerar_senha(tamanho: usize, classes: Classes) -> Result<String, ErroCripto> {
    let conjuntos = classes.conjuntos();
    if conjuntos.is_empty() || !(TAM_SENHA_MIN..=TAM_SENHA_MAX).contains(&tamanho) {
        return Err(ErroCripto::GeradorInvalido);
    }
    let alfabeto: Vec<u8> = conjuntos.concat();
    loop {
        let mut s = Vec::with_capacity(tamanho);
        for _ in 0..tamanho {
            s.push(alfabeto[uniforme(alfabeto.len())?]);
        }
        if conjuntos.iter().all(|c| s.iter().any(|b| c.contains(b))) {
            return Ok(String::from_utf8(s).expect("alfabeto ASCII"));
        }
        s.zeroize();
    }
}
