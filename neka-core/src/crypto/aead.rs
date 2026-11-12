use chacha20poly1305::aead::Aead;
use chacha20poly1305::aead::Payload;
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};

use super::{aleatorio, Chave, ErroCripto};

pub const TAM_NONCE: usize = 24;
pub const TAM_ETIQUETA: usize = 16;

/// Nonce de 192 bits (D9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nonce(pub [u8; TAM_NONCE]);

impl Nonce {
    /// Nonce aleatório novo. Deve ser gerado a cada cifragem.
    pub fn aleatorio() -> Result<Self, ErroCripto> {
        let mut n = [0; TAM_NONCE];
        aleatorio(&mut n)?;
        Ok(Self(n))
    }
}

fn cifra(chave: &Chave) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new_from_slice(chave.como_bytes()).expect("chave tem 32 bytes")
}

/// XChaCha20-Poly1305: devolve texto cifrado seguido da etiqueta de 16 bytes.
pub fn selar(
    chave: &Chave,
    nonce: &Nonce,
    aad: &[u8],
    claro: &[u8],
) -> Result<Vec<u8>, ErroCripto> {
    cifra(chave)
        .encrypt(&XNonce::from(nonce.0), Payload { msg: claro, aad })
        .map_err(|_| ErroCripto::TamanhoInvalido)
}

/// Inverso de [`selar`]. Qualquer alteração em chave, nonce, AAD ou texto
/// cifrado resulta em [`ErroCripto::FalhaAutenticacao`].
pub fn abrir(
    chave: &Chave,
    nonce: &Nonce,
    aad: &[u8],
    cifrado: &[u8],
) -> Result<Vec<u8>, ErroCripto> {
    if cifrado.len() < TAM_ETIQUETA {
        return Err(ErroCripto::FalhaAutenticacao);
    }
    cifra(chave)
        .decrypt(&XNonce::from(nonce.0), Payload { msg: cifrado, aad })
        .map_err(|_| ErroCripto::FalhaAutenticacao)
}

#[cfg(test)]
mod testes {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// draft-irtf-cfrg-xchacha-03, apêndice A.3.1.
    #[test]
    fn vetor_conhecido_xchacha20poly1305() {
        let chave = Chave::de_bytes(
            hex("808182838485868788898a8b8c8d8e8f909192939495969798999a9b9c9d9e9f")
                .try_into()
                .unwrap(),
        );
        let nonce = Nonce(
            hex("404142434445464748494a4b4c4d4e4f5051525354555657")
                .try_into()
                .unwrap(),
        );
        let aad = hex("50515253c0c1c2c3c4c5c6c7");
        let claro = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
        let esperado = hex(concat!(
            "bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb",
            "731c7f1b0b4aa6440bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b452",
            "2f8c9ba40db5d945b11b69b982c1bb9e3f3fac2bc369488f76b2383565d3fff9",
            "21f9664c97637da9768812f615c68b13b52e",
            "c0875924c1c7987947deafd8780acf49"
        ));
        let cifrado = selar(&chave, &nonce, &aad, claro).unwrap();
        assert_eq!(cifrado, esperado);
        assert_eq!(abrir(&chave, &nonce, &aad, &cifrado).unwrap(), claro);
    }
}
