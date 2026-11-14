//! Acceptance criteria of the cryptography (step 3).

use neka_core::crypto::*;
use neka_core::model::Uuid;

/// Minimum accepted params, so the tests are fast.
const FAST: KdfParams = KdfParams {
    mem_kib: KdfParams::MEM_MIN_KIB,
    iters: 1,
    lanes: 1,
};

fn vault() -> Uuid {
    Uuid::from_u128(0x1234)
}

#[test]
fn right_password_opens_and_wrong_fails() {
    let salt = Salt::random().unwrap();
    let data = Key::random().unwrap();
    let k = password_key(b"password fake", &salt, &FAST).unwrap();
    let e = wrap(&k, &data, &vault(), 0, SlotKind::Password).unwrap();

    let k2 = password_key(b"password fake", &salt, &FAST).unwrap();
    let open = unwrap(&k2, &e, &vault(), 0, SlotKind::Password).unwrap();
    assert_eq!(open.como_bytes(), data.como_bytes());

    let wrong = password_key(b"password fakE", &salt, &FAST).unwrap();
    assert_eq!(
        unwrap(&wrong, &e, &vault(), 0, SlotKind::Password).unwrap_err(),
        CryptoError::AuthenticationFailed
    );
}

#[test]
fn different_salt_or_params_change_the_key() {
    let s1 = Salt([1; 16]);
    let s2 = Salt([2; 16]);
    let a = password_key(b"x", &s1, &FAST).unwrap();
    let b = password_key(b"x", &s2, &FAST).unwrap();
    let c = password_key(b"x", &s1, &KdfParams { iters: 2, ..FAST }).unwrap();
    assert_ne!(a.como_bytes(), b.como_bytes());
    assert_ne!(a.como_bytes(), c.como_bytes());
}

#[test]
fn recovery_opens_its_own_slot() {
    let data = Key::random().unwrap();
    let rec = Key::random().unwrap();
    let k = recovery_key(&rec);
    let e = wrap(&k, &data, &vault(), 0, SlotKind::Recovery).unwrap();
    let open = unwrap(&recovery_key(&rec), &e, &vault(), 0, SlotKind::Recovery).unwrap();
    assert_eq!(open.como_bytes(), data.como_bytes());
    // the wrapping key is not the recovery key itself
    assert_ne!(k.como_bytes(), rec.como_bytes());
}

#[test]
fn aad_binds_vault_and_slot_kind() {
    let data = Key::random().unwrap();
    let k = Key::random().unwrap();
    let e = wrap(&k, &data, &vault(), 0, SlotKind::Password).unwrap();
    assert!(unwrap(&k, &e, &Uuid::from_u128(0x9999), 0, SlotKind::Password).is_err());
    assert!(unwrap(&k, &e, &vault(), 0, SlotKind::Recovery).is_err());
    // the key epoch also goes into the AAD (D15)
    assert!(unwrap(&k, &e, &vault(), 1, SlotKind::Password).is_err());
}

#[test]
fn any_changed_byte_in_the_slot_fails() {
    let data = Key::random().unwrap();
    let k = Key::random().unwrap();
    let e = wrap(&k, &data, &vault(), 0, SlotKind::Password).unwrap();
    for i in 0..WRAPPED_LEN {
        let mut m = e.clone();
        m.encrypted[i] ^= 0x01;
        assert!(
            unwrap(&k, &m, &vault(), 0, SlotKind::Password).is_err(),
            "byte {i}"
        );
    }
    for i in 0..NONCE_LEN {
        let mut m = e.clone();
        m.nonce.0[i] ^= 0x01;
        assert!(
            unwrap(&k, &m, &vault(), 0, SlotKind::Password).is_err(),
            "nonce {i}"
        );
    }
}

#[test]
fn seal_and_open_with_aad() {
    let k = Key::random().unwrap();
    let n = Nonce::random().unwrap();
    let c = seal(&k, &n, b"header", b"body fake").unwrap();
    assert_eq!(c.len(), 9 + TAG_LEN);
    assert_eq!(open(&k, &n, b"header", &c).unwrap(), b"body fake");
    assert!(open(&k, &n, b"headeR", &c).is_err());
    let mut t = c.clone();
    *t.last_mut().unwrap() ^= 1;
    assert!(open(&k, &n, b"header", &t).is_err());
    assert!(open(&k, &n, b"header", &c[..10]).is_err());
}

#[test]
fn random_nonces_and_keys_do_not_repeat() {
    let a = Nonce::random().unwrap();
    let b = Nonce::random().unwrap();
    assert_ne!(a, b);
    // wrapping the same key twice gives different nonces and texts
    let k = Key::random().unwrap();
    let d = Key::random().unwrap();
    let e1 = wrap(&k, &d, &vault(), 0, SlotKind::Password).unwrap();
    let e2 = wrap(&k, &d, &vault(), 0, SlotKind::Password).unwrap();
    assert_ne!(e1, e2);
}

#[test]
fn params_outside_the_limits_are_refused() {
    let bad = [
        KdfParams {
            mem_kib: KdfParams::MEM_MIN_KIB - 1,
            ..FAST
        },
        KdfParams {
            mem_kib: KdfParams::MEM_MAX_KIB + 1,
            ..FAST
        },
        KdfParams {
            mem_kib: u32::MAX,
            ..FAST
        },
        KdfParams { iters: 0, ..FAST },
        KdfParams { iters: 17, ..FAST },
        KdfParams { lanes: 0, ..FAST },
        KdfParams { lanes: 17, ..FAST },
    ];
    for p in bad {
        assert_eq!(
            password_key(b"x", &Salt([0; 16]), &p).unwrap_err(),
            CryptoError::ParamsOutOfLimits,
            "{p:?}"
        );
    }
    assert!(KdfParams::DEFAULT.validate().is_ok());
}

#[test]
fn debug_and_errors_do_not_leak_the_key() {
    let k = Key::de_bytes([0xAB; 32]);
    let s = format!("{k:?}");
    assert!(
        !s.contains("171") && !s.to_lowercase().contains("ab, "),
        "{s}"
    );
    for e in [CryptoError::AuthenticationFailed, CryptoError::KdfFailed] {
        assert!(!e.to_string().is_empty());
    }
}

#[test]
fn generator_respects_length_and_classes() {
    for _ in 0..200 {
        let s = generate_password(16, Classes::ALL).unwrap();
        assert_eq!(s.len(), 16);
        assert!(s.bytes().any(|b| b.is_ascii_lowercase()));
        assert!(s.bytes().any(|b| b.is_ascii_uppercase()));
        assert!(s.bytes().any(|b| b.is_ascii_digit()));
        assert!(s.bytes().any(|b| b.is_ascii_punctuation()));
    }
    let only_digits = Classes {
        lowercase: false,
        uppercase: false,
        digits: true,
        symbols: false,
    };
    assert!(generate_password(20, only_digits)
        .unwrap()
        .bytes()
        .all(|b| b.is_ascii_digit()));
    assert_ne!(
        generate_password(32, Classes::ALL).unwrap(),
        generate_password(32, Classes::ALL).unwrap()
    );
}

#[test]
fn generator_refuses_impossible_requests() {
    let nothing = Classes {
        lowercase: false,
        uppercase: false,
        digits: false,
        symbols: false,
    };
    assert_eq!(
        generate_password(16, nothing).unwrap_err(),
        CryptoError::InvalidGenerator
    );
    assert!(generate_password(PASSWORD_LEN_MIN - 1, Classes::ALL).is_err());
    assert!(generate_password(PASSWORD_LEN_MAX + 1, Classes::ALL).is_err());
}

#[test]
fn generator_distributes_without_gross_bias() {
    // 20 thousand digits: each should come out near 10% (generous tolerance).
    let only_digits = Classes {
        lowercase: false,
        uppercase: false,
        digits: true,
        symbols: false,
    };
    let mut count = [0u32; 10];
    for _ in 0..100 {
        for b in generate_password(200, only_digits).unwrap().bytes() {
            count[(b - b'0') as usize] += 1;
        }
    }
    for c in count {
        assert!((1700..2300).contains(&c), "{count:?}");
    }
}
