//! Recovery key text: base32 (RFC 4648, no padding) of the
//! 32 key bytes plus 2 check bytes, in groups of 5 separated
//! by hyphens. The check only catches typos; it is not security.

use sha2::{Digest, Sha256};
use zeroize::Zeroize;

use super::ApiError;
use crate::crypto::{Key, KEY_LEN};

const ALPHABET: &[u8; 32] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZ234567";
const CHECK_LEN: usize = 2;
const TOTAL_LEN: usize = KEY_LEN + CHECK_LEN;

fn check(key: &[u8; KEY_LEN]) -> [u8; CHECK_LEN] {
    let mut h = Sha256::new();
    h.update(b"neka-v1-recovery-check");
    h.update(key);
    let d = h.finalize();
    [d[0], d[1]]
}

/// Text to show to the user exactly once.
pub fn to_text(key: &Key) -> String {
    let mut data = [0u8; TOTAL_LEN];
    data[..KEY_LEN].copy_from_slice(key.como_bytes());
    data[KEY_LEN..].copy_from_slice(&check(key.como_bytes()));
    let mut bits: u32 = 0;
    let mut n = 0;
    let mut output = String::new();
    let mut count = 0;
    for &b in data.iter() {
        bits = (bits << 8) | b as u32;
        n += 8;
        while n >= 5 {
            n -= 5;
            if count > 0 && count % 5 == 0 {
                output.push('-');
            }
            output.push(ALPHABET[((bits >> n) & 31) as usize] as char);
            count += 1;
        }
    }
    if n > 0 {
        if count > 0 && count % 5 == 0 {
            output.push('-');
        }
        output.push(ALPHABET[((bits << (5 - n)) & 31) as usize] as char);
    }
    data.zeroize();
    output
}

/// Reads the typed text: ignores hyphens, spaces and letter case.
pub fn from_text(text: &str) -> Result<Key, ApiError> {
    let mut bits: u32 = 0;
    let mut n = 0;
    let mut data = Vec::with_capacity(TOTAL_LEN + 1);
    for c in text.chars().filter(|c| !c.is_whitespace() && *c != '-') {
        let c = c.to_ascii_uppercase();
        let v = ALPHABET
            .iter()
            .position(|&a| a as char == c)
            .ok_or(ApiError::InvalidRecoveryKey);
        let v = match v {
            Ok(v) => v as u32,
            Err(e) => {
                data.zeroize();
                return Err(e);
            }
        };
        bits = (bits << 5) | v;
        n += 5;
        if n >= 8 {
            n -= 8;
            data.push((bits >> n) as u8);
            if data.len() > TOTAL_LEN {
                data.zeroize();
                return Err(ApiError::InvalidRecoveryKey);
            }
        }
    }
    // leftover bits (fewer than 8) must be zero, and the exact size
    let ok = data.len() == TOTAL_LEN && (bits & ((1 << n) - 1)) == 0;
    let r = if ok {
        let mut k = [0u8; KEY_LEN];
        k.copy_from_slice(&data[..KEY_LEN]);
        let r = if data[KEY_LEN..] == check(&k) {
            Ok(Key::de_bytes(k))
        } else {
            Err(ApiError::InvalidRecoveryKey)
        };
        k.zeroize();
        r
    } else {
        Err(ApiError::InvalidRecoveryKey)
    };
    data.zeroize();
    r
}
