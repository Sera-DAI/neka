//! Acceptance criteria of storage (step 4), following the list
//! "Required tests" in the format specification.

use std::io;
use std::path::PathBuf;

use neka_core::crypto::{CryptoError, KdfParams, Key};
use neka_core::model::{Body, Entry, Uuid};
use neka_core::store::*;

const FAST: KdfParams = KdfParams {
    mem_kib: KdfParams::MEM_MIN_KIB,
    iters: 1,
    lanes: 1,
};
const PASSWORD: &[u8] = b"password master fake";

fn body() -> Body {
    let mut c = Body::default();
    let mut e = Entry::new(
        Uuid::from_u128(1),
        Uuid::from_u128(9),
        "exemplo".into(),
        1_700_000_000_000,
        1,
    );
    e.password = "password-fake-da-entry".into();
    c.entries.push(e);
    c.device_clock = 1;
    c
}

fn new() -> (OpenVault, Key, Vec<u8>) {
    let (mut c, rec) = OpenVault::create(PASSWORD, FAST, body()).unwrap();
    let b = c.serialize().unwrap();
    (c, rec, b)
}

/// Offsets in the v1 file.
const OFF_VERSION: usize = 4;
const OFF_FLAGS: usize = 6;
const OFF_EPOCH: usize = 24;
const OFF_MEM: usize = 29;
const OFF_COUNT: usize = 57;
const OFF_SLOTS: usize = OFF_COUNT + 1;
const SLOT_LEN: usize = 1 + 24 + 48;

#[test]
fn opens_with_password_and_recovers_the_body() {
    let (_, _, b) = new();
    assert_eq!(&b[..4], b"NEKA");
    let c = OpenVault::open_with_password(&b, PASSWORD).unwrap();
    assert_eq!(c.body(), &body());
}

#[test]
fn wrong_password_fails() {
    let (_, _, b) = new();
    let e = OpenVault::open_with_password(&b, b"password wrong").unwrap_err();
    assert_eq!(e, StorageError::Crypto(CryptoError::AuthenticationFailed));
}

#[test]
fn opens_with_recovery() {
    let (_, rec, b) = new();
    let c = OpenVault::open_with_recovery(&b, &rec).unwrap();
    assert_eq!(c.body(), &body());
    assert!(OpenVault::open_with_recovery(&b, &Key::random().unwrap()).is_err());
}

#[test]
fn any_changed_byte_makes_opening_fail() {
    let (_, rec, b) = new();
    for i in 0..b.len() {
        let mut m = b.clone();
        m[i] ^= 0x01;
        let by_password = OpenVault::open_with_password(&m, PASSWORD);
        let by_rec = OpenVault::open_with_recovery(&m, &rec);
        // A byte of a kind's slot can only prevent that kind;
        // in no case does the changed file open through both paths.
        assert!(
            by_password.is_err() || by_rec.is_err(),
            "byte {i} changed e o file open_failed pelos two caminhos"
        );
        // KDF params and salt only serve the password slot (D14):
        // changing them can only prevent that path. Otherwise, nothing opens.
        let of_password_slot = (OFF_MEM..OFF_COUNT).contains(&i);
        let in_slot = of_password_slot || (OFF_SLOTS..OFF_SLOTS + 2 * SLOT_LEN).contains(&i);
        if of_password_slot {
            assert!(by_password.is_err(), "byte {i}");
        }
        if !in_slot {
            assert!(by_password.is_err() && by_rec.is_err(), "byte {i}");
        }
    }
}

#[test]
fn corrupted_compartment_does_not_prevent_opening_via_the_other() {
    let (_, rec, b) = new();
    // corrupts the wrapped key of the password slot (1st slot)
    let mut m = b.clone();
    m[OFF_SLOTS + 1 + 24 + 5] ^= 0xFF;
    assert!(OpenVault::open_with_password(&m, PASSWORD).is_err());
    assert_eq!(
        OpenVault::open_with_recovery(&m, &rec).unwrap().body(),
        &body()
    );
    // corrupts the recovery one (2nd slot)
    let mut m = b.clone();
    m[OFF_SLOTS + SLOT_LEN + 1 + 24 + 5] ^= 0xFF;
    assert!(OpenVault::open_with_recovery(&m, &rec).is_err());
    assert_eq!(
        OpenVault::open_with_password(&m, PASSWORD).unwrap().body(),
        &body()
    );
}

#[test]
fn change_password_rotates_the_key_and_revokes_the_old_password_and_recovery() {
    let (mut c, old_rec, before) = new();
    assert_eq!(c.key_epoch_value(), 0);
    let new_rec = c.change_password(b"new password fake", FAST).unwrap();
    assert_eq!(c.key_epoch_value(), 1);
    let after = c.serialize().unwrap();

    // the epoch goes in the header and the encrypted body really changes
    assert_eq!(after[OFF_EPOCH..OFF_EPOCH + 4], 1u32.to_le_bytes());
    let start = OFF_SLOTS + 2 * SLOT_LEN;
    assert_ne!(
        before[start..],
        after[start..],
        "o body deveria ser recifrado"
    );

    // old password and old recovery do not open the new file
    assert!(OpenVault::open_with_password(&after, PASSWORD).is_err());
    assert!(OpenVault::open_with_recovery(&after, &old_rec).is_err());
    // new credentials open, with the same content
    let c2 = OpenVault::open_with_password(&after, b"new password fake").unwrap();
    assert_eq!(c2.body(), &body());
    assert_eq!(c2.key_epoch_value(), 1);
    assert!(OpenVault::open_with_recovery(&after, &new_rec).is_ok());

    // the old copy still opens with the old credentials (stated limit)
    assert!(OpenVault::open_with_password(&before, PASSWORD).is_ok());
    assert!(OpenVault::open_with_recovery(&before, &old_rec).is_ok());
}

#[test]
fn tampered_epoch_in_the_header_makes_opening_fail() {
    let (mut c, rec, _) = new();
    c.change_password(b"new password fake", FAST).unwrap();
    let b = c.serialize().unwrap();
    let mut m = b.clone();
    m[OFF_EPOCH..OFF_EPOCH + 4].copy_from_slice(&0u32.to_le_bytes());
    assert!(OpenVault::open_with_password(&m, b"new password fake").is_err());
    let _ = rec;
}

#[test]
fn changed_body_gets_a_new_nonce() {
    let (mut c, _, before) = new();
    c.body_mut().device_clock = 2;
    let after = c.serialize().unwrap();
    let start = OFF_SLOTS + 2 * SLOT_LEN;
    assert_ne!(before[start..start + 24], after[start..start + 24]);
    assert_eq!(
        OpenVault::open_with_password(&after, PASSWORD)
            .unwrap()
            .body()
            .device_clock,
        2
    );
}

#[test]
fn refuses_newer_version_flags_and_garbage() {
    let (_, _, b) = new();
    let mut m = b.clone();
    m[OFF_VERSION..OFF_VERSION + 2].copy_from_slice(&2u16.to_le_bytes());
    assert_eq!(
        OpenVault::open_with_password(&m, PASSWORD).unwrap_err(),
        StorageError::NewerVersion
    );
    let mut m = b.clone();
    m[OFF_FLAGS] = 1;
    assert_eq!(
        OpenVault::open_with_password(&m, PASSWORD).unwrap_err(),
        StorageError::UnknownFlags
    );
    for garbage in [&b""[..], b"NEKA", b"KEEPASS!", &b[..b.len() - 1]] {
        assert_eq!(
            OpenVault::open_with_password(garbage, PASSWORD).unwrap_err(),
            StorageError::InvalidFormat
        );
    }
    let mut m = b.clone();
    m.push(0);
    assert_eq!(
        OpenVault::open_with_password(&m, PASSWORD).unwrap_err(),
        StorageError::InvalidFormat
    );
}

#[test]
fn refuses_kdf_params_outside_the_limits_before_deriving() {
    let (_, _, b) = new();
    for mem in [1u32, KdfParams::MEM_MAX_KIB + 1, u32::MAX] {
        let mut m = b.clone();
        m[OFF_MEM..OFF_MEM + 4].copy_from_slice(&mem.to_le_bytes());
        assert_eq!(
            OpenVault::open_with_password(&m, PASSWORD).unwrap_err(),
            StorageError::Crypto(CryptoError::ParamsOutOfLimits)
        );
    }
}

#[test]
fn errors_do_not_leak_secrets() {
    let (c, _, b) = new();
    let s = format!(
        "{c:?} {}",
        OpenVault::open_with_password(&b, b"x").unwrap_err()
    );
    assert!(!s.contains("password-fake-da-entry") && !s.contains("password master"));
}

// ---- atomic write ----

fn folder() -> PathBuf {
    let d = std::env::temp_dir().join(format!("neka-teste-{}", Uuid::from_bytes(rand16())));
    std::fs::create_dir_all(&d).unwrap();
    d
}

/// `.tmp` files left behind in the folder.
fn leftovers(d: &std::path::Path) -> usize {
    std::fs::read_dir(d)
        .unwrap()
        .filter(|e| {
            e.as_ref()
                .unwrap()
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp")
        })
        .count()
}

fn rand16() -> [u8; 16] {
    let mut b = [0u8; 16];
    neka_core::crypto::random(&mut b).unwrap();
    b
}

#[test]
fn atomic_write_failing_at_each_step_preserves_the_original() {
    let (mut c, _, original) = new();
    c.body_mut().device_clock = 7;
    let new_bytes = c.serialize().unwrap();
    let steps = [Step::CreateTemp, Step::Write, Step::Sync, Step::Rename];
    for p in steps {
        let d = folder();
        let target = d.join("vault.neka");
        write_atomic(&target, &original).unwrap();
        let r = write_atomic_with_failure(&target, &new_bytes, |q| {
            if q == p {
                Err(io::Error::other("failure injected"))
            } else {
                Ok(())
            }
        });
        assert!(r.is_err(), "{p:?}");
        assert_eq!(
            read_file(&target).unwrap(),
            original,
            "original changed on failure at {p:?}"
        );
        assert_eq!(leftovers(&d), 0, "temp file left behind at {p:?}");
        assert!(OpenVault::open_with_password(&read_file(&target).unwrap(), PASSWORD).is_ok());
        std::fs::remove_dir_all(&d).unwrap();
    }
    // Failure to sync the folder, after the rename: the new file is already
    // whole in place, so the write counts as done.
    let d = folder();
    let target = d.join("vault.neka");
    write_atomic(&target, &original).unwrap();
    let r = write_atomic_with_failure(&target, &new_bytes, |q| {
        if q == Step::SyncFolder {
            Err(io::Error::other("failure injected"))
        } else {
            Ok(())
        }
    });
    assert!(r.is_ok());
    assert_eq!(read_file(&target).unwrap(), new_bytes);
    assert_eq!(leftovers(&d), 0);
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn write_without_failure_replaces_and_backup_copies() {
    let d = folder();
    let target = d.join("vault.neka");
    let (_, _, b) = new();
    write_atomic(&target, b"version old").unwrap();
    let bak = backup(&target).unwrap();
    assert_eq!(bak, d.join("vault.neka.bak"));
    write_atomic(&target, &b).unwrap();
    assert_eq!(read_file(&target).unwrap(), b);
    assert_eq!(read_file(&bak).unwrap(), b"version old");
    std::fs::remove_dir_all(&d).unwrap();
}

#[test]
fn oversized_vault_is_not_written() {
    // Hundreds of entries at the maximum size exceed 64 MiB once encrypted.
    let mut c = body();
    for n in 10..230u128 {
        let mut e = Entry::new(
            Uuid::from_u128(n),
            Uuid::from_u128(9),
            format!("e{n}"),
            1,
            1,
        );
        e.notes = "x".repeat(64 * 1024);
        for i in 0..64 {
            e.extra.insert(format!("k{i}"), "y".repeat(4096));
        }
        c.entries.push(e);
    }
    let (mut vault, _) = OpenVault::create(PASSWORD, FAST, c).unwrap();
    assert_eq!(vault.serialize().unwrap_err(), StorageError::FileTooLarge);
}

#[cfg(unix)]
#[test]
fn temp_file_starts_restricted_and_does_not_follow_a_planted_link() {
    use std::os::unix::fs::PermissionsExt;
    let d = folder();
    let target = d.join("vault.neka");
    write_atomic(&target, b"conteudo").unwrap();
    assert_eq!(
        std::fs::metadata(&target).unwrap().permissions().mode() & 0o777,
        0o600
    );
    // a planted symlink with the old temp file name is not followed
    let victim = d.join("victim.txt");
    std::fs::write(&victim, b"intacto").unwrap();
    std::os::unix::fs::symlink(&victim, d.join("vault.neka.tmp")).unwrap();
    write_atomic(&target, b"new").unwrap();
    assert_eq!(std::fs::read(&victim).unwrap(), b"intacto");
    std::fs::remove_dir_all(&d).unwrap();
}
