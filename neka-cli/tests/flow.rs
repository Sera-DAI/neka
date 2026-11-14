//! Real end-to-end use of the CLI, with two "devices" and a folder
//! that plays the role of Google Drive. Passwords go through standard input.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MASTER: &str = "password master fake 1";
const NEW: &str = "password master fake 2";

fn neka(dir: &Path, args: &[&str], entry: &str) -> (bool, String, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_neka"))
        .arg("--dir")
        .arg(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    // Commands that fail before reading input (for example, the screen without
    // a terminal) close the pipe: that is not a test error, the result is checked.
    let _ = c.stdin.take().unwrap().write_all(entry.as_bytes());
    let o = c.wait_with_output().unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

fn ok(dir: &Path, args: &[&str], entry: &str) -> String {
    let (s, out, err) = neka(dir, args, entry);
    assert!(s, "neka {args:?} failed:\n{out}\n{err}");
    out
}

fn base(name: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "neka-cli-{name}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(d.join("cloud")).unwrap();
    d
}

fn m(s: &str) -> String {
    format!("{s}\n")
}

#[test]
fn full_daily_use() {
    let d = base("use");
    let a = d.join("a");
    let b = d.join("b");
    let cloud = d.join("cloud");

    // create: password, repeat, Enter after noting down the key
    let out = ok(&a, &["create"], &format!("{MASTER}\n{MASTER}\n\n"));
    let key = out
        .lines()
        .map(str::trim)
        .find(|l| l.len() == 65 && l.matches('-').count() == 10)
        .expect("recovery key in the output")
        .to_owned();

    // wrong password fails
    let (s, _, err) = neka(&a, &["list"], "wrong\n");
    assert!(!s);
    assert!(err.contains("incorrect password"), "{err}");

    ok(
        &a,
        &[
            "add",
            "Bank",
            "-u",
            "ana",
            "--url",
            "https://bank.invalid",
            "--tag",
            "finance",
        ],
        &format!("{MASTER}\naccount-1\n"),
    );
    ok(&a, &["add", "Email", "--generate", "24"], &m(MASTER));
    let out = ok(&a, &["list"], &m(MASTER));
    assert!(out.contains("Bank") && out.contains("Email"), "{out}");
    let out = ok(&a, &["ls", "-t", "finance"], &m(MASTER));
    assert!(out.contains("Bank") && !out.contains("Email"), "{out}");

    let out = ok(&a, &["show", "bank"], &m(MASTER));
    assert!(
        !out.contains("account-1"),
        "password must not appear without --password"
    );
    let out = ok(&a, &["show", "bank", "--password"], &m(MASTER));
    assert!(out.contains("account-1"));

    ok(
        &a,
        &["edit", "Bank", "--password"],
        &format!("{MASTER}\naccount-2\n"),
    );
    let out = ok(&a, &["show", "Bank", "--password", "--history"], &m(MASTER));
    assert!(
        out.contains("Password:  account-2") && out.contains("old (") && out.contains("account-1"),
        "{out}"
    );

    // sync with a second device
    ok(&a, &["sync-folder", cloud.to_str().unwrap()], "");
    ok(&a, &["sync"], &m(MASTER));
    assert!(cloud.join("vault.neka").exists());
    ok(
        &b,
        &["clone", cloud.join("vault.neka").to_str().unwrap()],
        &m(MASTER),
    );
    ok(
        &b,
        &["add", "Store", "-u", "bia"],
        &format!("{MASTER}\naccount-3\n"),
    );
    let out = ok(&a, &["list"], &m(MASTER));
    assert!(
        out.contains("Store"),
        "A should see the entry from B: {out}"
    );

    // deletion propagates
    ok(&a, &["remove", "Email", "-y"], &m(MASTER));
    let out = ok(&b, &["list"], &m(MASTER));
    assert!(!out.contains("Email"), "{out}");

    // password change on A; B asks for the new one on sync
    ok(
        &a,
        &["master-password"],
        &format!("{MASTER}\n{NEW}\n{NEW}\n\n"),
    );
    let (s, out, err) = neka(&b, &["list"], &format!("{MASTER}\n{NEW}\n"));
    assert!(s, "{out}\n{err}");
    assert!(err.contains("changed on another device"), "{err}");
    let (s, _, _) = neka(&b, &["list", "--no-sync"], &m(MASTER));
    assert!(!s, "the old password can no longer open the vault of B");
    ok(&b, &["list"], &m(NEW));

    // the recovery key from creation stopped being valid after the change
    let (s, _, _) = neka(
        &a,
        &["recover", "--no-sync"],
        &format!("{key}\nxxxxxxxxxxxx\nxxxxxxxxxxxx\n\n"),
    );
    assert!(!s);

    // export and import
    let csv = d.join("export.csv");
    let (s, _, err) = neka(&a, &["export", csv.to_str().unwrap()], &m(NEW));
    assert!(!s && err.contains("--plaintext"));
    ok(
        &a,
        &["export", csv.to_str().unwrap(), "--plaintext"],
        &m(NEW),
    );
    let kdbx = d.join("export.kdbx");
    ok(
        &a,
        &["export", kdbx.to_str().unwrap()],
        &format!("{NEW}\nkeepass fake\nkeepass fake\n"),
    );
    let c = d.join("c");
    ok(&c, &["create"], &format!("{MASTER}\n{MASTER}\n\n"));
    let out = ok(
        &c,
        &["import", kdbx.to_str().unwrap()],
        &format!("keepass fake\n{MASTER}\n"),
    );
    assert!(out.contains("Imported: 2"), "{out}");
    let out = ok(&c, &["import", csv.to_str().unwrap()], &m(MASTER));
    assert!(out.contains("already existed: 2"), "{out}");

    // generate does not need a vault
    let out = ok(&d.join("empty"), &["generate", "30", "--no-symbols"], "");
    assert_eq!(out.trim().len(), 30);
    assert!(out.trim().chars().all(|c| c.is_ascii_alphanumeric()));
}

#[test]
fn recover_with_the_key() {
    let d = base("recover");
    let a = d.join("a");
    let out = ok(&a, &["create"], &format!("{MASTER}\n{MASTER}\n\n"));
    let key = out
        .lines()
        .map(str::trim)
        .find(|l| l.len() == 65 && l.matches('-').count() == 10)
        .unwrap()
        .to_lowercase();
    ok(&a, &["add", "X"], &format!("{MASTER}\nsx\n"));
    let out = ok(&a, &["recover"], &format!("{key}\n{NEW}\n{NEW}\n\n"));
    assert!(out.contains("Master password reset"));
    let out = ok(&a, &["ls"], &m(NEW));
    assert!(out.contains('X'));
}

#[test]
fn piped_output_is_plain_text_without_decoration() {
    let d = base("plain");
    let a = d.join("a");
    ok(&a, &["create"], &format!("{MASTER}\n{MASTER}\n\n"));
    ok(
        &a,
        &["add", "Bank", "-u", "ana"],
        &format!("{MASTER}\nsx\n"),
    );
    let (s, out, err) = neka(&a, &["list"], &m(MASTER));
    assert!(s);
    assert!(
        !out.contains('\u{1b}') && !err.contains('\u{1b}'),
        "without colors em pipe"
    );
    assert!(!out.contains('\u{2661}'), "without glyphs em pipe: {out}");
    assert!(out.contains("Bank  (ana)"), "{out}");
    assert!(err.contains("1 entry."), "{err}");
    let (s, _, err) = neka(&a, &["show", "nothing"], &m(MASTER));
    assert!(!s && err.contains("error: no entry matching"), "{err}");
}

#[test]
fn list_reveal_shows_password_and_password_date() {
    let d = base("reveal");
    let a = d.join("a");
    ok(&a, &["create"], &format!("{MASTER}\n{MASTER}\n\n"));
    ok(&a, &["add", "Bank"], &format!("{MASTER}\naccount-1\n"));
    let out = ok(&a, &["list"], &m(MASTER));
    assert!(
        !out.contains("account-1"),
        "without --reveal the password does not appear: {out}"
    );
    let out = ok(&a, &["list", "--reveal"], &m(MASTER));
    assert!(
        out.contains("password: account-1") && out.contains("set on: 20"),
        "{out}"
    );
    let out = ok(&a, &["show", "Bank"], &m(MASTER));
    assert!(out.contains("Set on:"), "{out}");
}

#[test]
fn interactive_screen_requires_a_terminal() {
    let d = base("screen");
    let a = d.join("a");
    ok(&a, &["create"], &format!("{MASTER}\n{MASTER}\n\n"));
    let (s, _, err) = neka(&a, &["ui"], &m(MASTER));
    assert!(!s && err.contains("needs a terminal"), "{err}");
}
