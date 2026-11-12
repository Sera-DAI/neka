//! Critérios de aceite da sincronização por pasta (Fase 2).

use std::fs;
use std::path::{Path, PathBuf};

use neka_core::api::*;

const RAPIDO: ParametrosKdf = ParametrosKdf {
    mem_kib: ParametrosKdf::MEM_MIN_KIB,
    iters: 1,
    lanes: 1,
};
const SENHA: &[u8] = b"senha mestra ficticia";
const NOVA: &[u8] = b"outra senha ficticia";
const A: Uuid = Uuid::from_u128(1);
const B: Uuid = Uuid::from_u128(2);
const T0: i64 = 1_700_000_000_000;

/// Pasta temporária com `a/`, `b/` (dispositivos) e `nuvem/`.
fn cenario() -> PathBuf {
    let mut r = [0u8; 16];
    neka_core::crypto::aleatorio(&mut r).unwrap();
    let d = std::env::temp_dir().join(format!("neka-sync-{}", Uuid::from_bytes(r)));
    for s in ["a", "b", "nuvem"] {
        fs::create_dir_all(d.join(s)).unwrap();
    }
    d
}

fn nova(t: &str, s: &str) -> NovaEntrada {
    NovaEntrada {
        title: t.into(),
        password: s.into(),
        ..Default::default()
    }
}

fn titulos(c: &Cofre) -> Vec<String> {
    let mut v: Vec<String> = c.entradas().iter().map(|e| e.title.clone()).collect();
    v.sort();
    v
}

fn arquivos(p: &Path) -> Vec<String> {
    let mut v: Vec<String> = fs::read_dir(p)
        .unwrap()
        .map(|e| e.unwrap().file_name().into_string().unwrap())
        .filter(|n| n.ends_with(".neka"))
        .collect();
    v.sort();
    v
}

/// A cria e publica; B clona da nuvem.
fn dois_dispositivos(d: &Path) -> (Cofre, Cofre) {
    let (mut a, _) = Cofre::criar(&d.join("a/cofre.neka"), A, SENHA, RAPIDO).unwrap();
    a.adicionar(nova("banco", "s1"), T0).unwrap();
    a.salvar().unwrap();
    a.sincronizar(&d.join("nuvem"), SENHA, None, T0).unwrap();
    let b = Cofre::clonar(
        &d.join("nuvem/cofre.neka"),
        &d.join("b/cofre.neka"),
        B,
        SENHA,
    )
    .unwrap();
    (a, b)
}

#[test]
fn edicoes_nos_dois_dispositivos_se_juntam() {
    let d = cenario();
    let (mut a, mut b) = dois_dispositivos(&d);
    assert_eq!(titulos(&b), ["banco"]);

    let id = b.entradas()[0].id;
    b.editar(id, |e| e.password = "s2".into(), T0 + 1).unwrap();
    b.adicionar(nova("email", "s3"), T0 + 1).unwrap();
    a.adicionar(nova("loja", "s4"), T0 + 2).unwrap();
    b.salvar().unwrap();
    a.salvar().unwrap();

    let rb = b
        .sincronizar(&d.join("nuvem"), SENHA, None, T0 + 3)
        .unwrap();
    assert!(rb.backup.exists());
    let ra = a
        .sincronizar(&d.join("nuvem"), SENHA, None, T0 + 4)
        .unwrap();
    assert!(ra.alterou_local);
    b.sincronizar(&d.join("nuvem"), SENHA, None, T0 + 5)
        .unwrap();

    assert_eq!(titulos(&a), ["banco", "email", "loja"]);
    assert_eq!(titulos(&a), titulos(&b));
    assert_eq!(a.entrada(id).unwrap().password, "s2");
    // persistiu no disco
    drop(a);
    let a = Cofre::abrir(&d.join("a/cofre.neka"), A, &Credencial::Senha(SENHA)).unwrap();
    assert_eq!(titulos(&a), ["banco", "email", "loja"]);
}

#[test]
fn exclusao_se_propaga() {
    let d = cenario();
    let (mut a, mut b) = dois_dispositivos(&d);
    let id = a.entradas()[0].id;
    a.remover(id, T0 + 1).unwrap();
    a.salvar().unwrap();
    a.sincronizar(&d.join("nuvem"), SENHA, None, T0 + 2)
        .unwrap();
    b.sincronizar(&d.join("nuvem"), SENHA, None, T0 + 3)
        .unwrap();
    assert!(b.entradas().is_empty());
}

#[test]
fn copia_de_conflito_e_absorvida_e_apagada() {
    let d = cenario();
    let (mut a, mut b) = dois_dispositivos(&d);
    b.adicionar(nova("email", "s3"), T0 + 1).unwrap();
    b.salvar().unwrap();
    // O Drive gravou a versão de B como cópia de conflito.
    fs::copy(d.join("b/cofre.neka"), d.join("nuvem/cofre (1).neka")).unwrap();
    let r = a
        .sincronizar(&d.join("nuvem"), SENHA, None, T0 + 2)
        .unwrap();
    assert_eq!(titulos(&a), ["banco", "email"]);
    assert_eq!(r.removidos.len(), 1);
    assert_eq!(arquivos(&d.join("nuvem")), ["cofre.neka"]);
}

#[test]
fn senha_trocada_em_outro_dispositivo() {
    let d = cenario();
    let (mut a, mut b) = dois_dispositivos(&d);
    let velho = fs::read(d.join("nuvem/cofre.neka")).unwrap();
    b.adicionar(nova("email", "s3"), T0 + 1).unwrap();
    b.salvar().unwrap();

    a.trocar_senha_mestra(NOVA, RAPIDO).unwrap();
    a.salvar().unwrap();
    a.sincronizar(&d.join("nuvem"), NOVA, None, T0 + 2).unwrap();

    // B ainda usa a senha antiga: a sincronização para sem mudar nada.
    let antes = fs::read(d.join("b/cofre.neka")).unwrap();
    let e = b
        .sincronizar(&d.join("nuvem"), SENHA, None, T0 + 3)
        .unwrap_err();
    assert_eq!(e, ErroApi::SenhaTrocadaEmOutroDispositivo);
    assert_eq!(fs::read(d.join("b/cofre.neka")).unwrap(), antes);

    // Com a senha nova, B adota a chave nova e mantém a entrada local.
    let r = b
        .sincronizar(&d.join("nuvem"), SENHA, Some(NOVA), T0 + 4)
        .unwrap();
    assert!(r.adotou_senha_nova);
    assert_eq!(titulos(&b), ["banco", "email"]);
    assert_eq!(b.epoca_da_chave(), a.epoca_da_chave());
    drop(b);
    assert!(Cofre::abrir(&d.join("b/cofre.neka"), B, &Credencial::Senha(SENHA)).is_err());
    let b = Cofre::abrir(&d.join("b/cofre.neka"), B, &Credencial::Senha(NOVA)).unwrap();
    assert_eq!(titulos(&b), ["banco", "email"]);

    // Uma cópia da época antiga não é mesclada nem apagada.
    fs::write(d.join("nuvem/cofre (2).neka"), &velho).unwrap();
    let r = a.sincronizar(&d.join("nuvem"), NOVA, None, T0 + 5).unwrap();
    assert_eq!(r.copias_velhas.len(), 1);
    assert!(d.join("nuvem/cofre (2).neka").exists());
    assert_eq!(titulos(&a), ["banco", "email"]);
}

#[test]
fn outro_cofre_na_pasta_e_ignorado() {
    let d = cenario();
    let (mut a, _b) = dois_dispositivos(&d);
    let (_x, _) = Cofre::criar(&d.join("nuvem/outro.neka"), A, SENHA, RAPIDO).unwrap();
    fs::write(d.join("nuvem/lixo.neka"), b"nao e cofre").unwrap();
    let r = a
        .sincronizar(&d.join("nuvem"), SENHA, None, T0 + 1)
        .unwrap();
    assert_eq!(r.ignorados.len(), 2);
    assert!(d.join("nuvem/outro.neka").exists());
}

#[test]
fn pasta_invalida_e_recusada() {
    let d = cenario();
    let (mut a, _) = Cofre::criar(&d.join("a/cofre.neka"), A, SENHA, RAPIDO).unwrap();
    assert_eq!(
        a.sincronizar(&d.join("a"), SENHA, None, T0).unwrap_err(),
        ErroApi::PastaDeSyncInvalida
    );
    assert_eq!(
        a.sincronizar(&d.join("nao-existe"), SENHA, None, T0)
            .unwrap_err(),
        ErroApi::PastaDeSyncInvalida
    );
}

#[test]
fn backups_sao_rotacionados() {
    let d = cenario();
    let (mut a, _) = Cofre::criar(&d.join("a/cofre.neka"), A, SENHA, RAPIDO).unwrap();
    for i in 0..(BACKUPS_MANTIDOS as i64 + 3) {
        a.sincronizar(&d.join("nuvem"), SENHA, None, T0 + i)
            .unwrap();
    }
    assert_eq!(arquivos(&d.join("a/backups")).len(), BACKUPS_MANTIDOS);
}
