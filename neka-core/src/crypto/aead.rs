use chacha20poly1305::aead::Aead;
use chacha20poly1305::aead::Payload;
use chacha20poly1305::{KeyInit, XChaCha20Poly1305, XNonce};

use super::{random, CryptoError, Key};

pub const NONCE_LEN: usize = 24;
pub const TAG_LEN: usize = 16;

/// 192-bit nonce (D9).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Nonce(pub [u8; NONCE_LEN]);

impl Nonce {
    /// New random nonce. Must be generated for every encryption.
    pub fn random() -> Result<Self, CryptoError> {
        let mut n = [0; NONCE_LEN];
        random(&mut n)?;
        Ok(Self(n))
    }
}

fn cipher(key: &Key) -> XChaCha20Poly1305 {
    XChaCha20Poly1305::new_from_slice(key.como_bytes()).expect("key has 32 bytes")
}

/// XChaCha20-Poly1305: returns ciphertext followed by the 16-byte tag.
pub fn seal(key: &Key, nonce: &Nonce, aad: &[u8], plain: &[u8]) -> Result<Vec<u8>, CryptoError> {
    cipher(key)
        .encrypt(&XNonce::from(nonce.0), Payload { msg: plain, aad })
        .map_err(|_| CryptoError::InvalidSize)
}

/// Inverse of [`seal`]. Any change to key, nonce, AAD or text
/// ciphertext results in [`CryptoError::AuthenticationFailed`].
pub fn open(
    key: &Key,
    nonce: &Nonce,
    aad: &[u8],
    encrypted: &[u8],
) -> Result<Vec<u8>, CryptoError> {
    if encrypted.len() < TAG_LEN {
        return Err(CryptoError::AuthenticationFailed);
    }
    cipher(key)
        .decrypt(
            &XNonce::from(nonce.0),
            Payload {
                msg: encrypted,
                aad,
            },
        )
        .map_err(|_| CryptoError::AuthenticationFailed)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// draft-irtf-cfrg-xchacha-03, appendix A.3.1.
    #[test]
    fn known_vector_xchacha20poly1305() {
        let key = Key::de_bytes(
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
        let plain = b"Ladies and Gentlemen of the class of '99: If I could offer you only one tip for the future, sunscreen would be it.";
        let expected = hex(concat!(
            "bd6d179d3e83d43b9576579493c0e939572a1700252bfaccbed2902c21396cbb",
            "731c7f1b0b4aa6440bf3a82f4eda7e39ae64c6708c54c216cb96b72e1213b452",
            "2f8c9ba40db5d945b11b69b982c1bb9e3f3fac2bc369488f76b2383565d3fff9",
            "21f9664c97637da9768812f615c68b13b52e",
            "c0875924c1c7987947deafd8780acf49"
        ));
        let encrypted = seal(&key, &nonce, &aad, plain).unwrap();
        assert_eq!(encrypted, expected);
        assert_eq!(open(&key, &nonce, &aad, &encrypted).unwrap(), plain);
    }
}
