use uuid::Uuid;
use zeroize::Zeroize;

use super::file::{File, Slot};
use super::StorageError;
use crate::crypto::{
    open, password_key, random, recovery_key, seal, unwrap, wrap, KdfParams, Key, Nonce, Salt,
    SlotKind,
};
use crate::model::Body;

/// Decrypted vault in memory.
///
/// Keeps the encrypted body from the last read or write: if the body was not
/// changed ([`OpenVault::body_mut`] was not called), the next write
/// reuses the same bytes. That way changing the password does not change the body.
pub struct OpenVault {
    file: File,
    data_key: Key,
    body: Body,
    changed: bool,
}

impl std::fmt::Debug for OpenVault {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CofreAberto")
            .field("vault_id", &self.file.vault_id)
            .field("secrets", &"<hidden>")
            .finish()
    }
}

fn random_uuid() -> Result<Uuid, StorageError> {
    let mut b = [0u8; 16];
    random(&mut b)?;
    Ok(uuid::Builder::from_random_bytes(b).into_uuid())
}

fn seal_body(
    key: &Key,
    vault_id: &Uuid,
    epoch: u32,
    body: &Body,
) -> Result<(Nonce, Vec<u8>), StorageError> {
    let mut plain = body.to_cbor()?;
    let nonce = Nonce::random()?;
    let len = (plain.len() + crate::crypto::TAG_LEN) as u64;
    let r = seal(
        key,
        &nonce,
        &File::aad_body(vault_id, epoch, &nonce, len),
        &plain,
    );
    plain.zeroize();
    Ok((nonce, r?))
}

impl OpenVault {
    /// Creates a new vault. Returns the vault and the recovery key, which must
    /// be shown exactly once and never written in plaintext.
    pub fn create(
        password: &[u8],
        kdf: KdfParams,
        body: Body,
    ) -> Result<(Self, Key), StorageError> {
        body.validate()?;
        let vault_id = random_uuid()?;
        let salt = Salt::random()?;
        let data_key = Key::random()?;
        let recovery = Key::random()?;
        let k_password = password_key(password, &salt, &kdf)?;
        let slots = vec![
            Slot {
                kind: SlotKind::Password,
                key: wrap(&k_password, &data_key, &vault_id, 0, SlotKind::Password)?,
            },
            Slot {
                kind: SlotKind::Recovery,
                key: wrap(
                    &recovery_key(&recovery),
                    &data_key,
                    &vault_id,
                    0,
                    SlotKind::Recovery,
                )?,
            },
        ];
        let (body_nonce, sealed) = seal_body(&data_key, &vault_id, 0, &body)?;
        let file = File {
            vault_id,
            key_epoch: 0,
            kdf,
            salt,
            slots,
            body_nonce,
            body: sealed,
        };
        Ok((
            Self {
                file,
                data_key,
                body,
                changed: false,
            },
            recovery,
        ))
    }

    pub fn open_with_password(bytes: &[u8], password: &[u8]) -> Result<Self, StorageError> {
        let file = File::decode(bytes)?;
        let k = password_key(password, &file.salt, &file.kdf)?;
        Self::open_with(file, &k, SlotKind::Password)
    }

    pub fn open_with_recovery(bytes: &[u8], recovery: &Key) -> Result<Self, StorageError> {
        let file = File::decode(bytes)?;
        Self::open_with(file, &recovery_key(recovery), SlotKind::Recovery)
    }

    fn open_with(file: File, wrap: &Key, kind: SlotKind) -> Result<Self, StorageError> {
        let slot = file
            .slots
            .iter()
            .find(|s| s.kind == kind)
            .ok_or(StorageError::NoCompartment)?;
        let data_key = unwrap(wrap, &slot.key, &file.vault_id, file.key_epoch, kind)?;
        let aad = File::aad_body(
            &file.vault_id,
            file.key_epoch,
            &file.body_nonce,
            file.body.len() as u64,
        );
        let mut plain = open(&data_key, &file.body_nonce, &aad, &file.body)?;
        let body = Body::from_cbor(&plain);
        plain.zeroize();
        Ok(Self {
            file,
            data_key,
            body: body?,
            changed: false,
        })
    }

    pub fn vault_id(&self) -> Uuid {
        self.file.vault_id
    }

    /// Data key generation (goes up with each master password change).
    pub fn key_epoch_value(&self) -> u32 {
        self.file.key_epoch
    }

    pub fn params_kdf(&self) -> KdfParams {
        self.file.kdf
    }

    pub fn body(&self) -> &Body {
        &self.body
    }

    /// Access to change the body; marks the body to be re-encrypted.
    pub fn body_mut(&mut self) -> &mut Body {
        self.changed = true;
        &mut self.body
    }

    /// Changes the master password **rotating the data key** (D15): new data key,
    /// epoch +1, new salt, new recovery key and body re-encrypted on the
    /// next write. Whoever has only the old password (or the old recovery)
    /// can no longer open the new file. Returns the new recovery key, to be
    /// shown exactly once. Nothing changes if any step fails.
    pub fn change_password(&mut self, new: &[u8], kdf: KdfParams) -> Result<Key, StorageError> {
        let epoch = self
            .file
            .key_epoch
            .checked_add(1)
            .ok_or(StorageError::InvalidFormat)?;
        let vault_id = self.file.vault_id;
        let salt = Salt::random()?;
        let k_password = password_key(new, &salt, &kdf)?;
        let data_key = Key::random()?;
        let recovery = Key::random()?;
        let slots = vec![
            Slot {
                kind: SlotKind::Password,
                key: wrap(&k_password, &data_key, &vault_id, epoch, SlotKind::Password)?,
            },
            Slot {
                kind: SlotKind::Recovery,
                key: wrap(
                    &recovery_key(&recovery),
                    &data_key,
                    &vault_id,
                    epoch,
                    SlotKind::Recovery,
                )?,
            },
        ];
        self.file.key_epoch = epoch;
        self.file.salt = salt;
        self.file.kdf = kdf;
        self.file.slots = slots;
        self.data_key = data_key; // the old one is zeroed when dropped
        self.changed = true; // the body is re-encrypted with the new key
        Ok(recovery)
    }

    /// File bytes to write. If the body changed, re-encrypts with a new nonce.
    pub fn serialize(&mut self) -> Result<Vec<u8>, StorageError> {
        if self.changed {
            let (nonce, body) = seal_body(
                &self.data_key,
                &self.file.vault_id,
                self.file.key_epoch,
                &self.body,
            )?;
            self.file.body_nonce = nonce;
            self.file.body = body;
            self.changed = false;
        }
        let bytes = self.file.encode();
        // Reading refuses files above the limit; writing a vault that
        // opening it later would be data loss.
        if bytes.len() > super::MAX_FILE_LEN {
            return Err(StorageError::FileTooLarge);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const FAST: KdfParams = KdfParams {
        mem_kib: KdfParams::MEM_MIN_KIB,
        iters: 1,
        lanes: 1,
    };

    /// Proves the revocation (D15) at the key level: after the change, the old data
    /// key neither decrypts the new body nor unwraps the new slots.
    #[test]
    fn old_data_key_does_not_decrypt_the_new_body() {
        let (mut c, _) = OpenVault::create(b"password fake", FAST, Body::default()).unwrap();
        let old = Key::de_bytes(*c.data_key.como_bytes());
        c.change_password(b"other password fake", FAST).unwrap();
        c.serialize().unwrap();
        assert_ne!(old.como_bytes(), c.data_key.como_bytes());
        let a = &c.file;
        let aad = File::aad_body(&a.vault_id, a.key_epoch, &a.body_nonce, a.body.len() as u64);
        assert!(open(&old, &a.body_nonce, &aad, &a.body).is_err());
        assert!(open(&c.data_key, &a.body_nonce, &aad, &a.body).is_ok());
    }

    /// If the change fails (invalid params), nothing changes.
    #[test]
    fn failed_change_does_not_alter_the_vault() {
        let (mut c, _) = OpenVault::create(b"password fake", FAST, Body::default()).unwrap();
        let before = c.serialize().unwrap();
        let bad = KdfParams { mem_kib: 1, ..FAST };
        assert!(c.change_password(b"x", bad).is_err());
        assert_eq!(c.key_epoch_value(), 0);
        assert_eq!(c.serialize().unwrap(), before);
    }
}
