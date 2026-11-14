use uuid::Uuid;

use super::{open, seal, CryptoError, Key, Nonce, KEY_LEN, TAG_LEN};

/// Encrypted data key (32) plus tag (16).
pub const WRAPPED_LEN: usize = KEY_LEN + TAG_LEN;

const AAD_PREFIX: &[u8] = b"neka-v1-keyslot";

/// Key slot kind (the format specification).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum SlotKind {
    Password = 1,
    Recovery = 2,
}

impl SlotKind {
    pub fn de_byte(b: u8) -> Option<Self> {
        match b {
            1 => Some(Self::Password),
            2 => Some(Self::Recovery),
            _ => None,
        }
    }
}

/// Result of wrapping: what is written into the slot.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct WrappedKey {
    pub nonce: Nonce,
    pub encrypted: [u8; WRAPPED_LEN],
}

fn aad(vault_id: &Uuid, epoch: u32, kind: SlotKind) -> Vec<u8> {
    let mut a = Vec::with_capacity(AAD_PREFIX.len() + 21);
    a.extend_from_slice(AAD_PREFIX);
    a.extend_from_slice(vault_id.as_bytes());
    a.extend_from_slice(&epoch.to_le_bytes());
    a.push(kind as u8);
    a
}

/// Wraps the data key with the wrapping key, with a new nonce.
/// AAD: `"neka-v1-keyslot" || vault_id || key_epoch || slot_type` (D15).
pub fn wrap(
    wrap: &Key,
    data: &Key,
    vault_id: &Uuid,
    epoch: u32,
    kind: SlotKind,
) -> Result<WrappedKey, CryptoError> {
    let nonce = Nonce::random()?;
    let c = seal(wrap, &nonce, &aad(vault_id, epoch, kind), data.como_bytes())?;
    let encrypted = c.try_into().map_err(|_| CryptoError::InvalidSize)?;
    Ok(WrappedKey { nonce, encrypted })
}

/// Recovers the data key. Wrong key, swapped vault, epoch or kind, or
/// any changed byte: [`CryptoError::AuthenticationFailed`].
pub fn unwrap(
    wrap: &Key,
    e: &WrappedKey,
    vault_id: &Uuid,
    epoch: u32,
    kind: SlotKind,
) -> Result<Key, CryptoError> {
    let mut plain = open(wrap, &e.nonce, &aad(vault_id, epoch, kind), &e.encrypted)?;
    let r = <[u8; KEY_LEN]>::try_from(plain.as_slice()).map_err(|_| CryptoError::InvalidSize);
    zeroize::Zeroize::zeroize(&mut plain);
    r.map(Key::de_bytes)
}
