use core::fmt;

use zeroize::{Zeroize, ZeroizeOnDrop};

use super::CryptoError;

pub const KEY_LEN: usize = 32;

/// 256-bit symmetric key. Zeroed when it goes out of scope; `Debug` does not show
/// the content; does not implement `Clone` to avoid stray copies.
#[derive(Zeroize, ZeroizeOnDrop)]
pub struct Key([u8; KEY_LEN]);

impl Key {
    pub fn de_bytes(bytes: [u8; KEY_LEN]) -> Self {
        Self(bytes)
    }

    /// New key with bytes from the operating system.
    pub fn random() -> Result<Self, CryptoError> {
        let mut k = Self([0; KEY_LEN]);
        random(&mut k.0)?;
        Ok(k)
    }

    pub fn como_bytes(&self) -> &[u8; KEY_LEN] {
        &self.0
    }
}

impl fmt::Debug for Key {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str("Key(<hidden>)")
    }
}

/// Fills `target` with random bytes from the operating system.
pub fn random(target: &mut [u8]) -> Result<(), CryptoError> {
    getrandom::fill(target).map_err(|_| CryptoError::RandomnessUnavailable)
}
