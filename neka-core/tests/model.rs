//! Acceptance criteria of the domain (step 2).

use neka_core::model::{Body, Entry, ModelError, Tombstone, Uuid, MAX_HISTORY};

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn entry(n: u128) -> Entry {
    let mut e = Entry::new(id(n), id(900), format!("site {n}"), 1_700_000_000_000, 1);
    e.username = "ana".into();
    e.password = "password-fake".into();
    e.url = "https://exemplo.invalid".into();
    e.notes = "note fake".into();
    e.tags = vec!["bank".into()];
    e.extra.insert("question".into(), "response fake".into());
    e
}

fn body() -> Body {
    let mut c = Body::default();
    c.entries.push(entry(1));
    c.entries.push(entry(2));
    c.tombstones.push(Tombstone {
        id: id(3),
        deleted_at: 1_700_000_001_000,
        clock: 5,
        device_id: id(900),
    });
    c.device_clock = 5;
    c
}

#[test]
fn cbor_round_trip_preserves_everything() {
    let c = body();
    let bytes = c.to_cbor().unwrap();
    assert_eq!(Body::from_cbor(&bytes).unwrap(), c);
}

#[test]
fn cbor_is_deterministic() {
    assert_eq!(body().to_cbor().unwrap(), body().to_cbor().unwrap());
}

#[test]
fn cbor_garbage_and_truncated_are_refused() {
    assert_eq!(
        Body::from_cbor(b"not e cbor").unwrap_err(),
        ModelError::InvalidCbor
    );
    let bytes = body().to_cbor().unwrap();
    assert!(Body::from_cbor(&bytes[..bytes.len() / 2]).is_err());
    assert!(Body::from_cbor(&[]).is_err());
}

#[test]
fn unknown_schema_is_refused() {
    let mut c = body();
    c.schema = 2;
    assert_eq!(c.validate().unwrap_err(), ModelError::UnknownSchema);
    assert!(c.to_cbor().is_err());
}

#[test]
fn duplicate_id_is_refused() {
    let mut c = body();
    c.entries.push(entry(1));
    assert_eq!(c.validate().unwrap_err(), ModelError::DuplicateId);
    // entry and tombstone with the same id too
    let mut c = body();
    c.tombstones[0].id = id(1);
    assert_eq!(c.validate().unwrap_err(), ModelError::DuplicateId);
}

#[test]
fn field_limits() {
    let mut e = entry(1);
    e.title = "  ".into();
    assert_eq!(e.validate().unwrap_err(), ModelError::EmptyTitle);
    let mut e = entry(1);
    e.title = "x".repeat(257);
    assert_eq!(e.validate().unwrap_err(), ModelError::TitleTooLong);
    let mut e = entry(1);
    e.password = "x".repeat(4097);
    assert_eq!(e.validate().unwrap_err(), ModelError::PasswordTooLong);
    let mut e = entry(1);
    e.tags = vec![String::new()];
    assert_eq!(e.validate().unwrap_err(), ModelError::InvalidTag);
    let mut e = entry(1);
    e.extra.insert(String::new(), "v".into());
    assert_eq!(e.validate().unwrap_err(), ModelError::InvalidExtra);
    assert!(entry(1).validate().is_ok());
}

#[test]
fn history_keeps_at_most_ten() {
    let mut e = entry(1);
    for i in 0..(MAX_HISTORY + 5) {
        e.change_password(format!("password-{i}"), 1_700_000_100_000 + i as i64);
    }
    assert_eq!(e.history.len(), MAX_HISTORY);
    // the oldest left first; the first original password is no longer there
    assert!(e.history.iter().all(|h| h.password != "password-fake"));
    assert_eq!(
        e.history.last().unwrap().password,
        format!("password-{}", MAX_HISTORY + 3)
    );
    assert!(e.validate().is_ok());
}

#[test]
fn debug_does_not_leak_secrets() {
    let mut e = entry(1);
    e.change_password("other-password-fake".into(), 1_700_000_100_000);
    let output = format!("{e:?} {:?}", body());
    for secret in [
        "password-fake",
        "note fake",
        "response fake",
        "other-password-fake",
    ] {
        assert!(!output.contains(secret), "leaked: {secret}");
    }
}

#[test]
fn errors_have_fixed_text_without_content() {
    let mut e = entry(1);
    e.password = "SECRET".repeat(1000);
    let msg = e.validate().unwrap_err().to_string();
    assert!(!msg.contains("SECRET"));
}
