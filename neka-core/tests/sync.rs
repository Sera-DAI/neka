//! Acceptance criteria of folder sync (Phase 2).

use std::fs;
use std::path::{Path, PathBuf};

use neka_core::api::*;

const FAST: KdfParams = KdfParams {
    mem_kib: KdfParams::MEM_MIN_KIB,
    iters: 1,
    lanes: 1,
};
const PASSWORD: &[u8] = b"password master fake";
const NEW: &[u8] = b"other password fake";
const A: Uuid = Uuid::from_u128(1);
const B: Uuid = Uuid::from_u128(2);
const T0: i64 = 1_700_000_000_000;

/// Temp folder with `a/`, `b/` (devices) and `cloud/`.
fn scenario() -> PathBuf {
    let mut r = [0u8; 16];
    neka_core::crypto::random(&mut r).unwrap();
    let d = std::env::temp_dir().join(format!("neka-sync-{}", Uuid::from_bytes(r)));
    for s in ["a", "b", "cloud"] {
        fs::create_dir_all(d.join(s)).unwrap();
    }
    d
}

fn new(t: &str, s: &str) -> NewEntry {
    NewEntry {
        title: t.into(),
        password: s.into(),
        ..Default::default()
    }
}

fn titles(c: &Vault) -> Vec<String> {
    let mut v: Vec<String> = c.entries().iter().map(|e| e.title.clone()).collect();
    v.sort();
    v
}

fn files(p: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(p)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".neka"))
        .collect();
    v.sort();
    v
}

/// A creates and publishes; B clones from the cloud.
fn two_devices(d: &Path) -> (Vault, Vault) {
    let (mut a, _) = Vault::create(&d.join("a/vault.neka"), A, PASSWORD, FAST).unwrap();
    a.add(new("bank", "s1"), T0).unwrap();
    a.save().unwrap();
    a.sync(&d.join("cloud"), PASSWORD, None, T0).unwrap();
    let b = Vault::clone(
        &d.join("cloud/vault.neka"),
        &d.join("b/vault.neka"),
        B,
        PASSWORD,
    )
    .unwrap();
    (a, b)
}

#[test]
fn edits_on_both_devices_merge() {
    let d = scenario();
    let (mut a, mut b) = two_devices(&d);
    assert_eq!(titles(&b), ["bank"]);

    let id = b.entries()[0].id;
    b.edit(id, |e| e.password = "s2".into(), T0 + 1).unwrap();
    b.add(new("email", "s3"), T0 + 1).unwrap();
    a.add(new("store", "s4"), T0 + 2).unwrap();
    b.save().unwrap();
    a.save().unwrap();

    let rb = b.sync(&d.join("cloud"), PASSWORD, None, T0 + 3).unwrap();
    assert!(rb.backup.exists());
    let ra = a.sync(&d.join("cloud"), PASSWORD, None, T0 + 4).unwrap();
    assert!(ra.changed_locally);
    b.sync(&d.join("cloud"), PASSWORD, None, T0 + 5).unwrap();

    assert_eq!(titles(&a), ["bank", "email", "store"]);
    assert_eq!(titles(&a), titles(&b));
    assert_eq!(a.entry(id).unwrap().password, "s2");
    // persisted to disk
    drop(a);
    let a = Vault::open(&d.join("a/vault.neka"), A, &Credential::Password(PASSWORD)).unwrap();
    assert_eq!(titles(&a), ["bank", "email", "store"]);
}

#[test]
fn deletion_propagates() {
    let d = scenario();
    let (mut a, mut b) = two_devices(&d);
    let id = a.entries()[0].id;
    a.remove(id, T0 + 1).unwrap();
    a.save().unwrap();
    a.sync(&d.join("cloud"), PASSWORD, None, T0 + 2).unwrap();
    b.sync(&d.join("cloud"), PASSWORD, None, T0 + 3).unwrap();
    assert!(b.entries().is_empty());
}

#[test]
fn conflict_copy_is_absorbed_and_deleted() {
    let d = scenario();
    let (mut a, mut b) = two_devices(&d);
    b.add(new("email", "s3"), T0 + 1).unwrap();
    b.save().unwrap();
    // Drive stored B's version as a conflict copy.
    fs::copy(d.join("b/vault.neka"), d.join("cloud/vault (1).neka")).unwrap();
    let r = a.sync(&d.join("cloud"), PASSWORD, None, T0 + 2).unwrap();
    assert_eq!(titles(&a), ["bank", "email"]);
    assert_eq!(r.removed.len(), 1);
    assert_eq!(files(&d.join("cloud")), ["vault.neka"]);
}

#[test]
fn password_changed_on_another_device() {
    let d = scenario();
    let (mut a, mut b) = two_devices(&d);
    let old = fs::read(d.join("cloud/vault.neka")).unwrap();
    b.add(new("email", "s3"), T0 + 1).unwrap();
    b.save().unwrap();

    a.change_master_password(NEW, FAST).unwrap();
    a.save().unwrap();
    a.sync(&d.join("cloud"), NEW, None, T0 + 2).unwrap();

    // B still uses the old password: the sync stops without changing anything.
    let before = fs::read(d.join("b/vault.neka")).unwrap();
    let e = b
        .sync(&d.join("cloud"), PASSWORD, None, T0 + 3)
        .unwrap_err();
    assert_eq!(e, ApiError::PasswordChangedOnOtherDevice);
    assert_eq!(fs::read(d.join("b/vault.neka")).unwrap(), before);

    // With the new password, B adopts the new key and keeps the local entry.
    let r = b
        .sync(&d.join("cloud"), PASSWORD, Some(NEW), T0 + 4)
        .unwrap();
    assert!(r.adopted_new_password);
    assert_eq!(titles(&b), ["bank", "email"]);
    assert_eq!(b.key_epoch_value(), a.key_epoch_value());
    drop(b);
    assert!(Vault::open(&d.join("b/vault.neka"), B, &Credential::Password(PASSWORD)).is_err());
    let b = Vault::open(&d.join("b/vault.neka"), B, &Credential::Password(NEW)).unwrap();
    assert_eq!(titles(&b), ["bank", "email"]);

    // A copy from the old epoch is neither merged nor deleted.
    fs::write(d.join("cloud/vault (2).neka"), &old).unwrap();
    let r = a.sync(&d.join("cloud"), NEW, None, T0 + 5).unwrap();
    assert_eq!(r.old_copies.len(), 1);
    assert!(d.join("cloud/vault (2).neka").exists());
    assert_eq!(titles(&a), ["bank", "email"]);
}

#[test]
fn another_vault_in_the_folder_is_ignored() {
    let d = scenario();
    let (mut a, _b) = two_devices(&d);
    let (_x, _) = Vault::create(&d.join("cloud/other.neka"), A, PASSWORD, FAST).unwrap();
    fs::write(d.join("cloud/garbage.neka"), b"not e vault").unwrap();
    let r = a.sync(&d.join("cloud"), PASSWORD, None, T0 + 1).unwrap();
    assert_eq!(r.ignored.len(), 2);
    assert!(d.join("cloud/other.neka").exists());
}

#[test]
fn invalid_folder_is_refused() {
    let d = scenario();
    let (mut a, _) = Vault::create(&d.join("a/vault.neka"), A, PASSWORD, FAST).unwrap();
    assert_eq!(
        a.sync(&d.join("a"), PASSWORD, None, T0).unwrap_err(),
        ApiError::InvalidSyncFolder
    );
    assert_eq!(
        a.sync(&d.join("not-exists"), PASSWORD, None, T0)
            .unwrap_err(),
        ApiError::InvalidSyncFolder
    );
}

#[test]
fn backups_are_rotated() {
    let d = scenario();
    let (mut a, _) = Vault::create(&d.join("a/vault.neka"), A, PASSWORD, FAST).unwrap();
    for i in 0..(BACKUPS_KEPT as i64 + 3) {
        a.sync(&d.join("cloud"), PASSWORD, None, T0 + i).unwrap();
    }
    assert_eq!(files(&d.join("a/backups")).len(), BACKUPS_KEPT);
}
