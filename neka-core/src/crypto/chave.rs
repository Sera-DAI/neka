use core::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop};

use super::ErroCripto;

pub const TAM_CHAVE: usize = 32;

/// Chave simétrica de 256 bits. Zerada ao sair de escopo; `Debug` não mostra
/// o conteúdo; não implementa `Clone` para evitar cópias soltas.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Chave([u8; TAM_CHAVE]);

impl Chave {
    pub fn de_bytes(bytes: [u8; TAM_CHAVE]) -> Self {
        Self(bytes)
    }

    /// Chave nova com bytes do sistema operacional.
    pub fn aleatoria() -> Result<Self, ErroCripto> {
        let mut k = Self([0; TAM_CHAVE]);
        aleatorio(&mut k.0)?;
        Ok(k)
    }

    pub fn como_bytes(&self) -> &[u8; TAM_CHAVE] {
        &self.0
    }
}

impl fmt::Debug for Chave {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Chave(<oculta>)")
    }
}

/// Preenche `destino` com bytes aleatórios do sistema operacional.
pub fn aleatorio(destino: &mut [u8]) -> Result<(), ErroCripto> {
    getrandom::fill(destino).map_err(|_| ErroCripto::AleatoriedadeIndisponivel)
}
