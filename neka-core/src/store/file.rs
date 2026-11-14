//! Binary encoding of `.neka` v1. Little-endian integers.

use uuid::Uuid;

use super::StorageError;
use crate::crypto::{
    KdfParams, Nonce, Salt, SlotKind, WrappedKey, NONCE_LEN, SALT_LEN, TAG_LEN, WRAPPED_LEN,
};

pub const MAGIC: [u8; 4] = *b"NEKA";
pub const CURRENT_VERSION: u16 = 1;
/// Size limit accepted when reading (64 MiB).
pub const MAX_FILE_LEN: usize = 64 * 1024 * 1024;

const KDF_ARGON2ID: u8 = 1;
const MAX_SLOTS: usize = 8;
/// magic + version + flags + vault_id + key_epoch: what the body AAD covers (D14, D15).
pub(crate) const PREFIX_LEN: usize = 4 + 2 + 2 + 16 + 4;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub kind: SlotKind,
    pub key: WrappedKey,
}

/// Decoded `.neka` file, still encrypted.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct File {
    pub vault_id: Uuid,
    /// Data key generation; goes up with each master password change (D15).
    pub key_epoch: u32,
    pub kdf: KdfParams,
    pub salt: Salt,
    pub slots: Vec<Slot>,
    pub body_nonce: Nonce,
    /// Encrypted body text, with the tag.
    pub body: Vec<u8>,
}

impl File {
    fn prefix(vault_id: &Uuid, epoch: u32) -> [u8; PREFIX_LEN] {
        let mut p = [0u8; PREFIX_LEN];
        p[..4].copy_from_slice(&MAGIC);
        p[4..6].copy_from_slice(&CURRENT_VERSION.to_le_bytes());
        p[6..8].copy_from_slice(&0u16.to_le_bytes());
        p[8..24].copy_from_slice(vault_id.as_bytes());
        p[24..28].copy_from_slice(&epoch.to_le_bytes());
        p
    }

    /// Body AAD (D14): fixed prefix, `body_nonce` and `body_len`.
    pub(crate) fn aad_body(vault_id: &Uuid, epoch: u32, nonce: &Nonce, body_len: u64) -> Vec<u8> {
        let mut a = Vec::with_capacity(PREFIX_LEN + NONCE_LEN + 8);
        a.extend_from_slice(&Self::prefix(vault_id, epoch));
        a.extend_from_slice(&nonce.0);
        a.extend_from_slice(&body_len.to_le_bytes());
        a
    }

    pub fn encode(&self) -> Vec<u8> {
        let mut o = Vec::with_capacity(128 + self.slots.len() * 73 + self.body.len());
        o.extend_from_slice(&Self::prefix(&self.vault_id, self.key_epoch));
        o.push(KDF_ARGON2ID);
        o.extend_from_slice(&self.kdf.mem_kib.to_le_bytes());
        o.extend_from_slice(&self.kdf.iters.to_le_bytes());
        o.extend_from_slice(&self.kdf.lanes.to_le_bytes());
        o.extend_from_slice(&self.salt.0);
        o.push(self.slots.len() as u8);
        for s in &self.slots {
            o.push(s.kind as u8);
            o.extend_from_slice(&s.key.nonce.0);
            o.extend_from_slice(&s.key.encrypted);
        }
        o.extend_from_slice(&self.body_nonce.0);
        o.extend_from_slice(&(self.body.len() as u64).to_le_bytes());
        o.extend_from_slice(&self.body);
        o
    }

    /// Decodes and validates the structure. Does not decrypt anything.
    pub fn decode(b: &[u8]) -> Result<Self, StorageError> {
        use StorageError::*;
        if b.len() > MAX_FILE_LEN {
            return Err(FileTooLarge);
        }
        let mut l = Reader { b, i: 0 };
        if l.read::<4>()? != MAGIC {
            return Err(InvalidFormat);
        }
        let version = u16::from_le_bytes(l.read()?);
        if version > CURRENT_VERSION {
            return Err(NewerVersion);
        }
        if version < CURRENT_VERSION {
            return Err(UnsupportedVersion);
        }
        if u16::from_le_bytes(l.read()?) != 0 {
            return Err(UnknownFlags);
        }
        let vault_id = Uuid::from_bytes(l.read()?);
        let key_epoch = u32::from_le_bytes(l.read()?);
        if l.read::<1>()?[0] != KDF_ARGON2ID {
            return Err(InvalidFormat);
        }
        let kdf = KdfParams {
            mem_kib: u32::from_le_bytes(l.read()?),
            iters: u32::from_le_bytes(l.read()?),
            lanes: u32::from_le_bytes(l.read()?),
        };
        kdf.validate()?;
        let salt = Salt(l.read::<SALT_LEN>()?);
        let n = l.read::<1>()?[0] as usize;
        if n == 0 || n > MAX_SLOTS {
            return Err(InvalidFormat);
        }
        let mut slots: Vec<Slot> = Vec::with_capacity(n);
        for _ in 0..n {
            let kind = SlotKind::de_byte(l.read::<1>()?[0]).ok_or(InvalidFormat)?;
            if slots.iter().any(|s| s.kind == kind) {
                return Err(InvalidFormat);
            }
            let nonce = Nonce(l.read()?);
            let encrypted = l.read::<WRAPPED_LEN>()?;
            slots.push(Slot {
                kind,
                key: WrappedKey { nonce, encrypted },
            });
        }
        let body_nonce = Nonce(l.read()?);
        let body_len = u64::from_le_bytes(l.read()?);
        let rest = b.len() - l.i;
        if body_len != rest as u64 || rest < TAG_LEN {
            return Err(InvalidFormat);
        }
        Ok(Self {
            vault_id,
            key_epoch,
            kdf,
            salt,
            slots,
            body_nonce,
            body: b[l.i..].to_vec(),
        })
    }
}

struct Reader<'a> {
    b: &'a [u8],
    i: usize,
}

impl Reader<'_> {
    fn read<const N: usize>(&mut self) -> Result<[u8; N], StorageError> {
        let end = self.i.checked_add(N).ok_or(StorageError::InvalidFormat)?;
        let s = self.b.get(self.i..end).ok_or(StorageError::InvalidFormat)?;
        self.i = end;
        Ok(s.try_into().expect("size conferido"))
    }
}
