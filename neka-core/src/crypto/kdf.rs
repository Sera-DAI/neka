use argon2::{Algorithm, Argon2, Params, Version};
use hkdf::Hkdf;
use sha2::Sha256;

use super::{random, CryptoError, Key, KEY_LEN};

pub const SALT_LEN: usize = 16;

/// HKDF context of the recovery key (the format specification).
const RECOVERY_CONTEXT: &[u8] = b"neka-v1-recovery";

/// Random salt of the master password.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Salt(pub [u8; SALT_LEN]);

impl Salt {
    pub fn random() -> Result<Self, CryptoError> {
        let mut s = [0; SALT_LEN];
        random(&mut s)?;
        Ok(Self(s))
    }
}

/// Argon2id params stored in the header (D8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct KdfParams {
    pub mem_kib: u32,
    pub iters: u32,
    pub lanes: u32,
}

impl KdfParams {
    /// Minimum accepted memory: 19 MiB (OWASP floor for Argon2id).
    pub const MEM_MIN_KIB: u32 = 19 * 1024;
    /// Maximum accepted memory: 1 GiB, so a malicious file cannot exhaust the machine.
    pub const MEM_MAX_KIB: u32 = 1024 * 1024;
    pub const ITERS_MIN: u32 = 1;
    pub const ITERS_MAX: u32 = 16;
    pub const LANES_MIN: u32 = 1;
    pub const LANES_MAX: u32 = 16;

    /// Initial proposal of D8: 256 MiB, 3 iterations, 4 threads.
    pub const DEFAULT: Self = Self {
        mem_kib: 256 * 1024,
        iters: 3,
        lanes: 4,
    };

    pub fn validate(&self) -> Result<(), CryptoError> {
        let ok = (Self::MEM_MIN_KIB..=Self::MEM_MAX_KIB).contains(&self.mem_kib)
            && (Self::ITERS_MIN..=Self::ITERS_MAX).contains(&self.iters)
            && (Self::LANES_MIN..=Self::LANES_MAX).contains(&self.lanes);
        if ok {
            Ok(())
        } else {
            Err(CryptoError::ParamsOutOfLimits)
        }
    }
}

/// Argon2id v0x13 without limit validation; internal use and test vectors.
fn argon2id_bruto(
    password: &[u8],
    salt: &[u8],
    secret: Option<&[u8]>,
    p: &KdfParams,
    output: &mut [u8],
) -> Result<(), CryptoError> {
    let params = Params::new(p.mem_kib, p.iters, p.lanes, Some(output.len()))
        .map_err(|_| CryptoError::ParamsOutOfLimits)?;
    let a = match secret {
        Some(s) => Argon2::new_with_secret(s, Algorithm::Argon2id, Version::V0x13, params)
            .map_err(|_| CryptoError::ParamsOutOfLimits)?,
        None => Argon2::new(Algorithm::Argon2id, Version::V0x13, params),
    };
    a.hash_password_into(password, salt, output)
        .map_err(|_| CryptoError::KdfFailed)
}

/// Wrapping key of the master password slot. Refuses params
/// outside the limits before allocating memory.
pub fn password_key(password: &[u8], salt: &Salt, p: &KdfParams) -> Result<Key, CryptoError> {
    p.validate()?;
    let mut output = [0u8; KEY_LEN];
    let r = argon2id_bruto(password, &salt.0, None, p, &mut output);
    let key = Key::de_bytes(output);
    zeroize::Zeroize::zeroize(&mut output);
    r.map(|_| key)
}

/// Wrapping key of the recovery slot:
/// HKDF-SHA256 without salt, with `info = "neka-v1-recovery"`.
pub fn recovery_key(recovery: &Key) -> Key {
    let mut output = [0u8; KEY_LEN];
    Hkdf::<Sha256>::new(None, recovery.como_bytes())
        .expand(RECOVERY_CONTEXT, &mut output)
        .expect("32 bytes cabem no HKDF-SHA256");
    let key = Key::de_bytes(output);
    zeroize::Zeroize::zeroize(&mut output);
    key
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

    /// RFC 9106, section 5.3 (Argon2id). Uses a secret and associated data, which
    /// neka does not use; that is why the test builds the context directly.
    #[test]
    fn known_vector_argon2id_rfc9106() {
        let params = Params::new(32, 3, 4, Some(32)).unwrap();
        let mut pb = argon2::ParamsBuilder::new();
        pb.m_cost(params.m_cost())
            .t_cost(3)
            .p_cost(4)
            .output_len(32)
            .data(argon2::AssociatedData::new(&[4u8; 12]).unwrap());
        let params = pb.build().unwrap();
        let a = Argon2::new_with_secret(&[3u8; 8], Algorithm::Argon2id, Version::V0x13, params)
            .unwrap();
        let mut output = [0u8; 32];
        a.hash_password_into(&[1u8; 32], &[2u8; 16], &mut output)
            .unwrap();
        assert_eq!(
            output.to_vec(),
            hex("0d640df58d78766c08c037a34a8b53c9d01ef0452d75b65eb52520e96b01e659")
        );
    }

    /// RFC 5869, test case 1 (HKDF-SHA256).
    #[test]
    fn known_vector_hkdf_rfc5869() {
        let mut okm = [0u8; 42];
        Hkdf::<Sha256>::new(Some(&hex("000102030405060708090a0b0c")), &[0x0b; 22])
            .expand(&hex("f0f1f2f3f4f5f6f7f8f9"), &mut okm)
            .unwrap();
        assert_eq!(
            okm.to_vec(),
            hex("3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865")
        );
    }

    #[test]
    fn raw_argon2id_is_deterministic() {
        let p = KdfParams {
            mem_kib: 64,
            iters: 1,
            lanes: 1,
        };
        let (mut a, mut b) = ([0u8; 32], [0u8; 32]);
        argon2id_bruto(b"x", &[7; 16], None, &p, &mut a).unwrap();
        argon2id_bruto(b"x", &[7; 16], None, &p, &mut b).unwrap();
        assert_eq!(a, b);
    }
}
