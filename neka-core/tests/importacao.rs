//! Critérios de aceite de importação e exportação (Fase 2, etapa 2).

use neka_core::api::*;

const RAPIDO: ParametrosKdf = ParametrosKdf {
    mem_kib: ParametrosKdf::MEM_MIN_KIB,
    iters: 1,
    lanes: 1,
};
const SENHA: &[u8] = b"senha mestra ficticia";
const T0: i64 = 1_700_000_000_000;

fn cofre() -> Cofre {
    let mut b = [0u8; 16];
    neka_core::crypto::aleatorio(&mut b).unwrap();
    let d = std::env::temp_dir().join(format!("neka-io-{}", Uuid::from_bytes(b)));
    std::fs::create_dir_all(&d).unwrap();
    Cofre::criar(&d.join("cofre.neka"), Uuid::from_u128(1), SENHA, RAPIDO)
        .unwrap()
        .0
}

fn reg(t: &str, u: &str, s: &str) -> Registro {
    let mut r = Registro::default();
    r.title = t.into();
    r.username = u.into();
    r.password = s.into();
    r
}

#[test]
fn csv_ida_e_volta_preserva_tudo() {
    let mut a = reg("Banco, \"o bom\"", "ana", "s;1,\"x\"");
    a.url = "https://exemplo.invalid".into();
    a.notes = "linha 1\nlinha 2".into();
    a.tags = vec!["financas".into(), "pessoal".into()];
    a.extra.insert("pin".into(), "1234".into());
    let b = reg("Email", "bia", "ç-ã-😀");
    let csv = gerar_csv(&[a.clone(), b.clone()]).unwrap();
    let lidos = ler_csv(&csv).unwrap();
    assert_eq!(lidos.len(), 2);
    assert!(lidos[0] == a, "primeiro registro mudou");
    assert!(lidos[1] == b, "segundo registro mudou");
}

#[test]
fn csv_do_bitwarden() {
    let csv = "folder,favorite,type,name,notes,fields,reprompt,login_uri,login_username,login_password,login_totp\n\
               Trabalho,,login,Git,,,0,https://git.invalid,ana,segredo,otpauth://totp/x\n";
    let r = ler_csv(csv.as_bytes()).unwrap();
    assert_eq!(r.len(), 1);
    assert_eq!(r[0].title, "Git");
    assert_eq!(r[0].username, "ana");
    assert_eq!(r[0].password, "segredo");
    assert_eq!(r[0].url, "https://git.invalid");
    assert_eq!(r[0].tags, ["Trabalho"]);
    assert_eq!(
        r[0].extra.get("totp").map(String::as_str),
        Some("otpauth://totp/x")
    );
}

#[test]
fn csv_do_chrome_e_do_keepassxc() {
    let chrome =
        "\u{feff}name,url,username,password,note\nloja.invalid,https://loja.invalid/,bia,abc,\n";
    let r = ler_csv(chrome.as_bytes()).unwrap();
    assert_eq!(
        (r[0].title.as_str(), r[0].password.as_str()),
        ("loja.invalid", "abc")
    );
    let kp = "\"Group\",\"Title\",\"Username\",\"Password\",\"URL\",\"Notes\"\n\"Raiz/Casa\",\"Wifi\",\"\",\"w1f1\",\"\",\"roteador\"\n";
    let r = ler_csv(kp.as_bytes()).unwrap();
    assert_eq!(r[0].title, "Wifi");
    assert_eq!(r[0].notes, "roteador");
    assert_eq!(r[0].tags, ["Raiz/Casa"]);
}

#[test]
fn csv_sem_colunas_conhecidas_e_recusado() {
    assert_eq!(ler_csv(b"a,b\n1,2\n").unwrap_err(), ErroIo::CsvSemColunas);
}

#[test]
fn importar_pula_duplicadas_e_invalidas() {
    let mut c = cofre();
    let mut grande = reg("Grande", "", "x");
    grande.notes = "n".repeat(70 * 1024);
    let regs = [
        reg("A", "u", "1"),
        reg("A", "u", "1"),
        grande,
        reg("B", "u", "2"),
    ];
    let r = c.importar(&regs, T0).unwrap();
    assert_eq!(
        r,
        RelatorioImportacao {
            adicionadas: 2,
            duplicadas: 1,
            invalidas: 1
        }
    );
    // importar de novo não duplica
    assert_eq!(c.importar(&regs[..1], T0).unwrap().duplicadas, 1);
    assert_eq!(c.entradas().len(), 2);
}

#[test]
fn kdbx_ida_e_volta() {
    let mut a = reg("Banco", "ana", "s3nh@");
    a.url = "https://banco.invalid".into();
    a.notes = "nota".into();
    a.tags = vec!["financas".into()];
    a.extra.insert("pin".into(), "1234".into());
    let bytes = gerar_kdbx(
        &[a.clone(), reg("Email", "bia", "x")],
        "exportacao ficticia",
    )
    .unwrap();
    assert_eq!(&bytes[..4], &[0x03, 0xd9, 0xa2, 0x9a], "assinatura KeePass");
    assert_eq!(
        ler_kdbx(&bytes, "errada").unwrap_err(),
        ErroIo::KdbxNaoAbriu
    );
    let mut lidos = ler_kdbx(&bytes, "exportacao ficticia").unwrap();
    lidos.sort_by(|x, y| x.title.cmp(&y.title));
    assert_eq!(lidos.len(), 2);
    assert!(lidos[0] == a, "registro mudou na ida e volta");
}

#[test]
fn exportar_do_cofre_e_importar_em_outro() {
    let mut c = cofre();
    c.importar(&[reg("A", "u", "1"), reg("B", "v", "2")], T0)
        .unwrap();
    let csv = gerar_csv(&c.registros()).unwrap();
    let mut d = cofre();
    assert_eq!(
        d.importar(&ler_csv(&csv).unwrap(), T0).unwrap().adicionadas,
        2
    );
}
