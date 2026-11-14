use zeroize::Zeroize;

use super::{random, CryptoError};

pub const PASSWORD_LEN_MIN: usize = 8;
pub const PASSWORD_LEN_MAX: usize = 256;

const LOWERCASE: &[u8] = b"abcdefghijklmnopqrstuvwxyz";
const UPPERCASE: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ";
const DIGITS: &[u8] = b"0123456789";
const SYMBOLS: &[u8] = b"!#$%&()*+,-./:;<=>?@[]^_{|}~";

/// Requested character classes. Each marked class appears at least once.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Classes {
    pub lowercase: bool,
    pub uppercase: bool,
    pub digits: bool,
    pub symbols: bool,
}

impl Classes {
    pub const ALL: Self = Self {
        lowercase: true,
        uppercase: true,
        digits: true,
        symbols: true,
    };

    fn sets(&self) -> Vec<&'static [u8]> {
        [
            (self.lowercase, LOWERCASE),
            (self.uppercase, UPPERCASE),
            (self.digits, DIGITS),
            (self.symbols, SYMBOLS),
        ]
        .into_iter()
        .filter_map(|(on, c)| on.then_some(c))
        .collect()
    }
}

/// Uniform integer in `0..n` by rejection (no modulo bias).
fn uniform(n: usize) -> Result<usize, CryptoError> {
    debug_assert!(n > 0 && n <= 256);
    let limit = 256 - (256 % n);
    loop {
        let mut b = [0u8; 1];
        random(&mut b)?;
        if (b[0] as usize) < limit {
            return Ok(b[0] as usize % n);
        }
    }
}

/// Generates a random password with the operating system as the source.
/// Draws until it contains all the requested classes (rejection, no bias).
pub fn generate_password(size: usize, classes: Classes) -> Result<String, CryptoError> {
    let sets = classes.sets();
    if sets.is_empty() || !(PASSWORD_LEN_MIN..=PASSWORD_LEN_MAX).contains(&size) {
        return Err(CryptoError::InvalidGenerator);
    }
    let alphabet: Vec<u8> = sets.concat();
    loop {
        let mut s = Vec::with_capacity(size);
        for _ in 0..size {
            s.push(alphabet[uniform(alphabet.len())?]);
        }
        if sets.iter().all(|c| s.iter().any(|b| c.contains(b))) {
            return Ok(String::from_utf8(s).expect("alphabet ASCII"));
        }
        s.zeroize();
    }
}
