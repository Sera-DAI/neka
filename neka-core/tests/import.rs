//! Acceptance criteria of import and export (Phase 2, step 2).

use neka_core::api::*;

const FAST: KdfParams = KdfParams {
    mem_kib: KdfParams::MEM_MIN_KIB,
    iters: 1,
    lanes: 1,
};
const PASSWORD: &[u8] = b"password master fake";
const T0: i64 = 1_700_000_000_000;

fn vault() -> Vault {
    let mut b = [0u8; 16];
    neka_core::crypto::random(&mut b).unwrap();
    let d = std::env::temp_dir().join(format!("neka-io-{}", Uuid::from_bytes(b)));
    std::fs::create_dir_all(&d).unwrap();
    Vault::create(&d.join("vault.neka"), Uuid::from_u128(1), PASSWORD, FAST)
        .unwrap()
        .0
}

fn reg(t: &str, u: &str, s: &str) -> Record {
    let mut r = Record::default();
    r.title = t.into();
    r.username = u.into();
    r.password = s.into();
    r
}

#[test]
fn csv_round_trip_preserves_everything() {
    let mut a = reg("Bank, \"o bom\"", "ana", "s;1,\"x\"");
    a.url = "https://exemplo.invalid".into();
    a.notes = "line 1\nlinha 2".into();
    a.tags = vec!["finance".into(), "personal".into()];
    a.extra.insert("pin".into(), "1234".into());
    let b = reg("Email", "bia", "ç-ã-😀");
    let csv = generate_csv(&[a.clone(), b.clone()]).unwrap();
    let read = read_csv(&csv).unwrap();
    assert_eq!(read.len(), 2);
    assert!(read[0] == a, "first record changed");
    assert!(read[1] == b, "second record changed");
}

#[test]
fn bitwarden_csv() {
    let csv = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
               Work,,login,Git,,,0,https://git.invalid,ana,secret,otpauth://totp/x\n";
    let r = read_csv(csv.as_bytes()).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].title, "Git");
    assert_eq!(r[0].username, "ana");
    assert_eq!(r[0].password, "secret");
    assert_eq!(r[0].url, "https://git.invalid");
    assert_eq!(r[0].tags, ["Work"]);
    assert_eq!(
        r[0].extra.get("totp").map(String::as_str),
        Some("otpauth://totp/x")
    );
}

#[test]
fn chrome_and_keepassxc_csv() {
    let chrome =
        "\u{feff}name,url,username,password,note\nstore.invalid,https://store.invalid/,bia,abc,\n";
    let r = read_csv(chrome.as_bytes()).unwrap();
    assert_eq!(
        (r[0].title.as_str(), r[0].password.as_str()),
        ("store.invalid", "abc")
    );
    let kp = "\"Group\",\"Title\",\"Username\",\"Password\",\"URL\",\"Notes\"\n\"Root/Home\",\"Wifi\",\"\",\"w1f1\",\"\",\"router\"\n";
    let r = read_csv(kp.as_bytes()).unwrap();
    assert_eq!(r[0].title, "Wifi");
    assert_eq!(r[0].notes, "router");
    assert_eq!(r[0].tags, ["Root/Home"]);
}

#[test]
fn csv_without_known_columns_is_refused() {
    assert_eq!(read_csv(b"a,b\n1,2\n").unwrap_err(), IoError::CsvNoColumns);
}

#[test]
fn import_skips_duplicates_and_invalid() {
    let mut c = vault();
    let mut large = reg("Large", "", "x");
    large.notes = "n".repeat(70 * 1024);
    let regs = [
        reg("A", "u", "1"),
        reg("A", "u", "1"),
        large,
        reg("B", "u", "2"),
    ];
    let r = c.import(&regs, T0).unwrap();
    assert_eq!(
        r,
        ImportReport {
            added: 2,
            duplicates: 1,
            invalid: 1
        }
    );
    // importing again does not duplicate
    assert_eq!(c.import(&regs[..1], T0).unwrap().duplicates, 1);
    assert_eq!(c.entries().len(), 2);
}

#[test]
fn kdbx_round_trip() {
    let mut a = reg("Bank", "ana", "s3nh@");
    a.url = "https://bank.invalid".into();
    a.notes = "note".into();
    a.tags = vec!["finance".into()];
    a.extra.insert("pin".into(), "1234".into());
    let bytes = generate_kdbx(&[a.clone(), reg("Email", "bia", "x")], "export fake").unwrap();
    assert_eq!(&bytes[..4], &[0x03, 0xd9, 0xa2, 0x9a], "signature KeePass");
    assert_eq!(
        read_kdbx(&bytes, "wrong").unwrap_err(),
        IoError::KdbxOpenFailed
    );
    let mut read = read_kdbx(&bytes, "export fake").unwrap();
    read.sort_by(|x, y| x.title.cmp(&y.title));
    assert_eq!(read.len(), 2);
    assert!(read[0] == a, "record changed na ida e back");
}

#[test]
fn export_from_the_vault_and_import_into_another() {
    let mut c = vault();
    c.import(&[reg("A", "u", "1"), reg("B", "v", "2")], T0)
        .unwrap();
    let csv = generate_csv(&c.records()).unwrap();
    let mut d = vault();
    assert_eq!(d.import(&read_csv(&csv).unwrap(), T0).unwrap().added, 2);
}
