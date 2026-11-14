//! Acceptance criteria of the API (step 5).

use std::path::PathBuf;

use neka_core::api::*;

const FAST: KdfParams = KdfParams {
    mem_kib: KdfParams::MEM_MIN_KIB,
    iters: 1,
    lanes: 1,
};
const PASSWORD: &[u8] = b"password master fake";
const DISP: Uuid = Uuid::from_u128(7);
const T0: i64 = 1_700_000_000_000;

fn path() -> PathBuf {
    let mut b = [0u8; 16];
    neka_core::crypto::random(&mut b).unwrap();
    let d = std::env::temp_dir().join(format!("neka-api-{}", Uuid::from_bytes(b)));
    std::fs::create_dir_all(&d).unwrap();
    d.join("vault.neka")
}

fn new(title: &str) -> NewEntry {
    NewEntry {
        title: title.into(),
        username: "ana".into(),
        password: "password-fake-1".into(),
        ..Default::default()
    }
}

#[test]
fn create_add_save_and_reopen() {
    let p = path();
    let (mut c, rec) = Vault::create(&p, DISP, PASSWORD, FAST).unwrap();
    let id = c.add(new("bank"), T0).unwrap();
    c.save().unwrap();
    drop(c); // the vault lock is only released when it is closed

    let r = Vault::open(&p, DISP, &Credential::Password(PASSWORD)).unwrap();
    assert_eq!(r.entries().len(), 1);
    assert_eq!(r.entry(id).unwrap().title, "bank");
    drop(r);

    let text = recovery_key_to_text(&rec);
    let r2 = Vault::open(&p, DISP, &Credential::Recovery(&text)).unwrap();
    assert_eq!(r2.entries().len(), 1);
}

#[test]
fn does_not_overwrite_existing_vault_and_refuses_empty_password() {
    let p = path();
    Vault::create(&p, DISP, PASSWORD, FAST).unwrap();
    assert_eq!(
        Vault::create(&p, DISP, PASSWORD, FAST).unwrap_err(),
        ApiError::FileAlreadyExists
    );
    assert_eq!(
        Vault::create(&path(), DISP, b"", FAST).unwrap_err(),
        ApiError::EmptyPassword
    );
}

#[test]
fn wrong_password_has_a_stable_code_without_leaking() {
    let p = path();
    Vault::create(&p, DISP, PASSWORD, FAST).unwrap();
    let e = Vault::open(&p, DISP, &Credential::Password(b"other")).unwrap_err();
    assert_eq!(e.code(), "incorrect_credential_or_tampered_file");
    assert!(!e.to_string().contains("other"));
    let e = Vault::open(&p, DISP, &Credential::Recovery("ABCDE-FGHIJ")).unwrap_err();
    assert_eq!(e, ApiError::InvalidRecoveryKey);
}

#[test]
fn editing_the_password_keeps_history_and_bumps_the_clock() {
    let p = path();
    let (mut c, _) = Vault::create(&p, DISP, PASSWORD, FAST).unwrap();
    let id = c.add(new("site"), T0).unwrap();
    let clock1 = c.entry(id).unwrap().clock;
    c.edit(
        id,
        |e| {
            e.password = "password-fake-2".into();
            e.notes = "note".into();
        },
        T0 + 1000,
    )
    .unwrap();
    let e = c.entry(id).unwrap();
    assert_eq!(e.password, "password-fake-2");
    assert_eq!(e.history.len(), 1);
    assert_eq!(e.history[0].password, "password-fake-1");
    assert_eq!(e.updated_at, T0 + 1000);
    assert_eq!(e.created_at, T0);
    assert!(e.clock > clock1);
    // editing without touching the password creates no history
    c.edit(id, |e| e.notes = "other".into(), T0 + 2000).unwrap();
    assert_eq!(c.entry(id).unwrap().history.len(), 1);
}

#[test]
fn invalid_edit_changes_nothing() {
    let p = path();
    let (mut c, _) = Vault::create(&p, DISP, PASSWORD, FAST).unwrap();
    let id = c.add(new("site"), T0).unwrap();
    let before = c.entry(id).unwrap().clone();
    let e = c.edit(id, |e| e.title = "   ".into(), T0 + 1).unwrap_err();
    assert_eq!(e.code(), "invalid_data");
    assert_eq!(c.entry(id).unwrap(), &before);
    assert_eq!(
        c.edit(Uuid::from_u128(99), |_| {}, T0).unwrap_err(),
        ApiError::EntryNotFound
    );
}

#[test]
fn id_does_not_change_on_edit() {
    let p = path();
    let (mut c, _) = Vault::create(&p, DISP, PASSWORD, FAST).unwrap();
    let id = c.add(new("site"), T0).unwrap();
    c.edit(id, |e| e.id = Uuid::from_u128(1), T0 + 1).unwrap();
    assert_eq!(c.entries().len(), 1);
    assert_eq!(c.entries()[0].id, id);
}

#[test]
fn remove_leaves_a_tombstone_and_persists() {
    let p = path();
    let (mut c, _) = Vault::create(&p, DISP, PASSWORD, FAST).unwrap();
    let id = c.add(new("site"), T0).unwrap();
    c.remove(id, T0 + 5).unwrap();
    assert!(c.entries().is_empty());
    assert_eq!(c.remove(id, T0 + 6).unwrap_err(), ApiError::EntryNotFound);
    c.save().unwrap();
    drop(c);
    let r = Vault::open(&p, DISP, &Credential::Password(PASSWORD)).unwrap();
    assert!(r.entries().is_empty());
}

#[test]
fn change_master_password_revokes_old_password_and_recovery() {
    let p = path();
    let (mut c, old_rec) = Vault::create(&p, DISP, PASSWORD, FAST).unwrap();
    c.add(new("site"), T0).unwrap();
    c.save().unwrap();
    assert_eq!(c.key_epoch_value(), 0);
    assert_eq!(
        c.change_master_password(b"", FAST).unwrap_err(),
        ApiError::EmptyPassword
    );
    let new_rec = c
        .change_master_password(b"new password fake", FAST)
        .unwrap();
    assert_eq!(c.key_epoch_value(), 1);
    c.save().unwrap();
    drop(c);

    assert!(Vault::open(&p, DISP, &Credential::Password(PASSWORD)).is_err());
    let old = recovery_key_to_text(&old_rec);
    assert!(Vault::open(&p, DISP, &Credential::Recovery(&old)).is_err());
    let r = Vault::open(&p, DISP, &Credential::Password(b"new password fake")).unwrap();
    assert_eq!(r.entries().len(), 1);
    assert_eq!(r.key_epoch_value(), 1);
    drop(r);
    let new_txt = recovery_key_to_text(&new_rec);
    let r = Vault::open(&p, DISP, &Credential::Recovery(&new_txt)).unwrap();
    assert_eq!(r.entries().len(), 1);
}

#[test]
fn recovery_key_round_trip_and_tolerance() {
    let k = Key::random().unwrap();
    let t = recovery_key_to_text(&k);
    assert_eq!(t.len(), 55 + 10); // 55 letters and 10 hyphens
    assert!(t.split('-').all(|g| g.len() == 5));
    assert_eq!(
        recovery_key_from_text(&t).unwrap().como_bytes(),
        k.como_bytes()
    );
    let typed = t.to_lowercase().replace('-', "  ");
    assert_eq!(
        recovery_key_from_text(&typed).unwrap().como_bytes(),
        k.como_bytes()
    );
}

#[test]
fn recovery_key_detects_typos() {
    let k = Key::random().unwrap();
    let t = recovery_key_to_text(&k);
    // swaps each letter for another and checks that all are refused
    for i in (0..t.len()).filter(|&i| t.as_bytes()[i] != b'-') {
        let mut b = t.clone().into_bytes();
        b[i] = if b[i] == b'A' { b'B' } else { b'A' };
        let r = recovery_key_from_text(std::str::from_utf8(&b).unwrap());
        assert!(r.is_err(), "position {i}");
    }
    assert!(recovery_key_from_text("").is_err());
    assert!(recovery_key_from_text(&t[..t.len() - 1]).is_err());
    assert!(recovery_key_from_text(&format!("{t}A")).is_err());
    assert!(recovery_key_from_text("1111-0000-!!!!").is_err());
}

#[test]
fn vault_debug_does_not_leak_content() {
    let p = path();
    let (mut c, _) = Vault::create(&p, DISP, PASSWORD, FAST).unwrap();
    c.add(new("site"), T0).unwrap();
    let s = format!("{c:?} {:?}", c.entries());
    assert!(!s.contains("password-fake-1"));
}

#[test]
fn generator_exposed_by_the_api() {
    assert_eq!(generate_password(20, Classes::ALL).unwrap().len(), 20);
    assert!(!VERSION.is_empty());
}

#[test]
fn two_instances_do_not_open_the_same_vault_at_once() {
    let p = path();
    let (c, _) = Vault::create(&p, DISP, PASSWORD, FAST).unwrap();
    assert_eq!(
        Vault::open(&p, DISP, &Credential::Password(PASSWORD)).unwrap_err(),
        ApiError::VaultInUse
    );
    drop(c);
    assert!(Vault::open(&p, DISP, &Credential::Password(PASSWORD)).is_ok());
}

#[test]
fn huge_recovery_text_is_refused() {
    let huge = "A".repeat(1_000_000);
    assert!(recovery_key_from_text(&huge).is_err());
}
