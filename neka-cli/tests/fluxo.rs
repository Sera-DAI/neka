//! Uso real da CLI de ponta a ponta, com dois "dispositivos" e uma pasta
//! que faz o papel do Google Drive. As senhas vão pela entrada padrão.

use std::io::Write;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

const MESTRA: &str = "senha mestra ficticia 1";
const NOVA: &str = "senha mestra ficticia 2";

fn neka(dir: &Path, args: &[&str], entrada: &str) -> (bool, String, String) {
    let mut c = Command::new(env!("CARGO_BIN_EXE_neka"))
        .arg("--dir")
        .arg(dir)
        .args(args)
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped())
        .spawn()
        .unwrap();
    c.stdin
        .take()
        .unwrap()
        .write_all(entrada.as_bytes())
        .unwrap();
    let o = c.wait_with_output().unwrap();
    (
        o.status.success(),
        String::from_utf8_lossy(&o.stdout).into_owned(),
        String::from_utf8_lossy(&o.stderr).into_owned(),
    )
}

fn ok(dir: &Path, args: &[&str], entrada: &str) -> String {
    let (s, out, err) = neka(dir, args, entrada);
    assert!(s, "neka {args:?} failed:\n{out}\n{err}");
    out
}

fn base(nome: &str) -> PathBuf {
    let d = std::env::temp_dir().join(format!(
        "neka-cli-{nome}-{}-{}",
        std::process::id(),
        std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .unwrap()
            .as_nanos()
    ));
    std::fs::create_dir_all(d.join("nuvem")).unwrap();
    d
}

fn m(s: &str) -> String {
    format!("{s}\n")
}

#[test]
fn uso_diario_completo() {
    let d = base("uso");
    let a = d.join("a");
    let b = d.join("b");
    let nuvem = d.join("nuvem");

    // criar: senha, repetir, Enter depois de anotar a chave
    let out = ok(&a, &["create"], &format!("{MESTRA}\n{MESTRA}\n\n"));
    let chave = out
        .lines()
        .map(str::trim)
        .find(|l| l.len() == 65 && l.matches('-').count() == 10)
        .expect("chave de recuperação na saída")
        .to_owned();

    // senha errada falha
    let (s, _, err) = neka(&a, &["list"], "errada\n");
    assert!(!s);
    assert!(err.contains("incorrect password"), "{err}");

    ok(
        &a,
        &[
            "add",
            "Banco",
            "-u",
            "ana",
            "--url",
            "https://banco.invalid",
            "--tag",
            "financas",
        ],
        &format!("{MESTRA}\nconta-1\n"),
    );
    ok(&a, &["add", "Email", "--generate", "24"], &m(MESTRA));
    let out = ok(&a, &["list"], &m(MESTRA));
    assert!(out.contains("Banco") && out.contains("Email"), "{out}");
    let out = ok(&a, &["ls", "-t", "financas"], &m(MESTRA));
    assert!(out.contains("Banco") && !out.contains("Email"), "{out}");

    let out = ok(&a, &["show", "banco"], &m(MESTRA));
    assert!(
        !out.contains("conta-1"),
        "password must not appear without --password"
    );
    let out = ok(&a, &["show", "banco", "--password"], &m(MESTRA));
    assert!(out.contains("conta-1"));

    ok(
        &a,
        &["edit", "Banco", "--password"],
        &format!("{MESTRA}\nconta-2\n"),
    );
    let out = ok(
        &a,
        &["show", "Banco", "--password", "--history"],
        &m(MESTRA),
    );
    assert!(
        out.contains("Password:  conta-2") && out.contains("old (") && out.contains("conta-1"),
        "{out}"
    );

    // sincronização com um segundo dispositivo
    ok(&a, &["sync-folder", nuvem.to_str().unwrap()], "");
    ok(&a, &["sync"], &m(MESTRA));
    assert!(nuvem.join("cofre.neka").exists());
    ok(
        &b,
        &["clone", nuvem.join("cofre.neka").to_str().unwrap()],
        &m(MESTRA),
    );
    ok(
        &b,
        &["add", "Loja", "-u", "bia"],
        &format!("{MESTRA}\nconta-3\n"),
    );
    let out = ok(&a, &["list"], &m(MESTRA));
    assert!(out.contains("Loja"), "A should see the entry from B: {out}");

    // exclusão propaga
    ok(&a, &["remove", "Email", "-y"], &m(MESTRA));
    let out = ok(&b, &["list"], &m(MESTRA));
    assert!(!out.contains("Email"), "{out}");

    // troca de senha em A; B pede a nova na sincronização
    ok(
        &a,
        &["master-password"],
        &format!("{MESTRA}\n{NOVA}\n{NOVA}\n\n"),
    );
    let (s, out, err) = neka(&b, &["list"], &format!("{MESTRA}\n{NOVA}\n"));
    assert!(s, "{out}\n{err}");
    assert!(err.contains("changed on another device"), "{err}");
    let (s, _, _) = neka(&b, &["list", "--no-sync"], &m(MESTRA));
    assert!(!s, "a senha antiga não pode mais abrir o cofre de B");
    ok(&b, &["list"], &m(NOVA));

    // a chave de recuperação da criação deixou de valer depois da troca
    let (s, _, _) = neka(
        &a,
        &["recover", "--no-sync"],
        &format!("{chave}\nxxxxxxxxxxxx\nxxxxxxxxxxxx\n\n"),
    );
    assert!(!s);

    // exportar e importar
    let csv = d.join("export.csv");
    let (s, _, err) = neka(&a, &["export", csv.to_str().unwrap()], &m(NOVA));
    assert!(!s && err.contains("--plaintext"));
    ok(
        &a,
        &["export", csv.to_str().unwrap(), "--plaintext"],
        &m(NOVA),
    );
    let kdbx = d.join("export.kdbx");
    ok(
        &a,
        &["export", kdbx.to_str().unwrap()],
        &format!("{NOVA}\nkeepass ficticia\nkeepass ficticia\n"),
    );
    let c = d.join("c");
    ok(&c, &["create"], &format!("{MESTRA}\n{MESTRA}\n\n"));
    let out = ok(
        &c,
        &["import", kdbx.to_str().unwrap()],
        &format!("keepass ficticia\n{MESTRA}\n"),
    );
    assert!(out.contains("Imported: 2"), "{out}");
    let out = ok(&c, &["import", csv.to_str().unwrap()], &m(MESTRA));
    assert!(out.contains("already existed: 2"), "{out}");

    // gerar não precisa de cofre
    let out = ok(&d.join("vazio"), &["generate", "30", "--no-symbols"], "");
    assert_eq!(out.trim().len(), 30);
    assert!(out.trim().chars().all(|c| c.is_ascii_alphanumeric()));
}

#[test]
fn recuperar_com_a_chave() {
    let d = base("recuperar");
    let a = d.join("a");
    let out = ok(&a, &["create"], &format!("{MESTRA}\n{MESTRA}\n\n"));
    let chave = out
        .lines()
        .map(str::trim)
        .find(|l| l.len() == 65 && l.matches('-').count() == 10)
        .unwrap()
        .to_lowercase();
    ok(&a, &["add", "X"], &format!("{MESTRA}\nsx\n"));
    let out = ok(&a, &["recover"], &format!("{chave}\n{NOVA}\n{NOVA}\n\n"));
    assert!(out.contains("Master password reset"));
    let out = ok(&a, &["ls"], &m(NOVA));
    assert!(out.contains('X'));
}

#[test]
fn nomes_antigos_em_portugues_continuam_valendo() {
    let d = base("alias");
    let a = d.join("a");
    ok(&a, &["criar"], &format!("{MESTRA}\n{MESTRA}\n\n"));
    ok(
        &a,
        &["adicionar", "Banco", "-u", "ana", "--etiqueta", "x"],
        &format!("{MESTRA}\nsx\n"),
    );
    let out = ok(&a, &["listar", "--sem-sync"], &m(MESTRA));
    assert!(out.contains("Banco"), "{out}");
    let out = ok(&a, &["mostrar", "banco", "--senha"], &m(MESTRA));
    assert!(out.contains("Password:  sx"), "{out}");
    ok(&a, &["remover", "Banco", "-s"], &m(MESTRA));
    let out = ok(&a, &["gerar", "12", "--sem-simbolos"], "");
    assert_eq!(out.trim().len(), 12);
}

#[test]
fn saida_em_pipe_e_texto_puro_sem_enfeites() {
    let d = base("puro");
    let a = d.join("a");
    ok(&a, &["create"], &format!("{MESTRA}\n{MESTRA}\n\n"));
    ok(
        &a,
        &["add", "Banco", "-u", "ana"],
        &format!("{MESTRA}\nsx\n"),
    );
    let (s, out, err) = neka(&a, &["list"], &m(MESTRA));
    assert!(s);
    assert!(
        !out.contains('\u{1b}') && !err.contains('\u{1b}'),
        "sem cores em pipe"
    );
    assert!(!out.contains('\u{2661}'), "sem glifos em pipe: {out}");
    assert!(out.contains("Banco  (ana)"), "{out}");
    assert!(err.contains("1 entry."), "{err}");
    let (s, _, err) = neka(&a, &["show", "nada"], &m(MESTRA));
    assert!(!s && err.contains("error: no entry matching"), "{err}");
}

#[test]
fn list_reveal_mostra_senha_e_data_da_senha() {
    let d = base("reveal");
    let a = d.join("a");
    ok(&a, &["create"], &format!("{MESTRA}\n{MESTRA}\n\n"));
    ok(&a, &["add", "Banco"], &format!("{MESTRA}\nconta-1\n"));
    let out = ok(&a, &["list"], &m(MESTRA));
    assert!(
        !out.contains("conta-1"),
        "sem --reveal a senha não aparece: {out}"
    );
    let out = ok(&a, &["list", "--reveal"], &m(MESTRA));
    assert!(
        out.contains("password: conta-1") && out.contains("set on: 20"),
        "{out}"
    );
    let out = ok(&a, &["show", "Banco"], &m(MESTRA));
    assert!(out.contains("Set on:"), "{out}");
}

#[test]
fn tela_interativa_exige_terminal() {
    let d = base("tela");
    let a = d.join("a");
    ok(&a, &["create"], &format!("{MESTRA}\n{MESTRA}\n\n"));
    let (s, _, err) = neka(&a, &["ui"], &m(MESTRA));
    assert!(!s && err.contains("needs a terminal"), "{err}");
}
