//! Critérios de aceite da API (etapa 5).

use std::path::PathBuf;

use neka_core::api::*;

const RAPIDO: ParametrosKdf = ParametrosKdf {
    mem_kib: ParametrosKdf::MEM_MIN_KIB,
    iters: 1,
    lanes: 1,
};
const SENHA: &[u8] = b"senha mestra ficticia";
const DISP: Uuid = Uuid::from_u128(7);
const T0: i64 = 1_700_000_000_000;

fn caminho() -> PathBuf {
    let mut b = [0u8; 16];
    neka_core::crypto::aleatorio(&mut b).unwrap();
    let d = std::env::temp_dir().join(format!("neka-api-{}", Uuid::from_bytes(b)));
    std::fs::create_dir_all(&d).unwrap();
    d.join("cofre.neka")
}

fn nova(titulo: &str) -> NovaEntrada {
    NovaEntrada {
        title: titulo.into(),
        username: "ana".into(),
        password: "senha-ficticia-1".into(),
        ..Default::default()
    }
}

#[test]
fn criar_adicionar_salvar_e_reabrir() {
    let p = caminho();
    let (mut c, rec) = Cofre::criar(&p, DISP, SENHA, RAPIDO).unwrap();
    let id = c.adicionar(nova("banco"), T0).unwrap();
    c.salvar().unwrap();
    drop(c); // a trava do cofre só cai quando ele é fechado

    let r = Cofre::abrir(&p, DISP, &Credencial::Senha(SENHA)).unwrap();
    assert_eq!(r.entradas().len(), 1);
    assert_eq!(r.entrada(id).unwrap().title, "banco");
    drop(r);

    let texto = chave_de_recuperacao_para_texto(&rec);
    let r2 = Cofre::abrir(&p, DISP, &Credencial::Recuperacao(&texto)).unwrap();
    assert_eq!(r2.entradas().len(), 1);
}

#[test]
fn nao_sobrescreve_cofre_existente_e_recusa_empty_password() {
    let p = caminho();
    Cofre::criar(&p, DISP, SENHA, RAPIDO).unwrap();
    assert_eq!(
        Cofre::criar(&p, DISP, SENHA, RAPIDO).unwrap_err(),
        ErroApi::ArquivoJaExiste
    );
    assert_eq!(
        Cofre::criar(&caminho(), DISP, b"", RAPIDO).unwrap_err(),
        ErroApi::SenhaVazia
    );
}

#[test]
fn senha_errada_tem_codigo_estavel_sem_vazar() {
    let p = caminho();
    Cofre::criar(&p, DISP, SENHA, RAPIDO).unwrap();
    let e = Cofre::abrir(&p, DISP, &Credencial::Senha(b"outra")).unwrap_err();
    assert_eq!(e.codigo(), "incorrect_credential_or_tampered_file");
    assert!(!e.to_string().contains("outra"));
    let e = Cofre::abrir(&p, DISP, &Credencial::Recuperacao("ABCDE-FGHIJ")).unwrap_err();
    assert_eq!(e, ErroApi::ChaveRecuperacaoInvalida);
}

#[test]
fn editar_troca_senha_guarda_historico_e_sobe_o_relogio() {
    let p = caminho();
    let (mut c, _) = Cofre::criar(&p, DISP, SENHA, RAPIDO).unwrap();
    let id = c.adicionar(nova("site"), T0).unwrap();
    let clock1 = c.entrada(id).unwrap().clock;
    c.editar(
        id,
        |e| {
            e.password = "senha-ficticia-2".into();
            e.notes = "nota".into();
        },
        T0 + 1000,
    )
    .unwrap();
    let e = c.entrada(id).unwrap();
    assert_eq!(e.password, "senha-ficticia-2");
    assert_eq!(e.history.len(), 1);
    assert_eq!(e.history[0].password, "senha-ficticia-1");
    assert_eq!(e.updated_at, T0 + 1000);
    assert_eq!(e.created_at, T0);
    assert!(e.clock > clock1);
    // editar sem mexer na senha não cria histórico
    c.editar(id, |e| e.notes = "outra".into(), T0 + 2000)
        .unwrap();
    assert_eq!(c.entrada(id).unwrap().history.len(), 1);
}

#[test]
fn edicao_invalida_nao_altera_nada() {
    let p = caminho();
    let (mut c, _) = Cofre::criar(&p, DISP, SENHA, RAPIDO).unwrap();
    let id = c.adicionar(nova("site"), T0).unwrap();
    let antes = c.entrada(id).unwrap().clone();
    let e = c
        .editar(id, |e| e.title = "   ".into(), T0 + 1)
        .unwrap_err();
    assert_eq!(e.codigo(), "invalid_data");
    assert_eq!(c.entrada(id).unwrap(), &antes);
    assert_eq!(
        c.editar(Uuid::from_u128(99), |_| {}, T0).unwrap_err(),
        ErroApi::EntradaNaoEncontrada
    );
}

#[test]
fn id_nao_muda_por_edicao() {
    let p = caminho();
    let (mut c, _) = Cofre::criar(&p, DISP, SENHA, RAPIDO).unwrap();
    let id = c.adicionar(nova("site"), T0).unwrap();
    c.editar(id, |e| e.id = Uuid::from_u128(1), T0 + 1).unwrap();
    assert_eq!(c.entradas().len(), 1);
    assert_eq!(c.entradas()[0].id, id);
}

#[test]
fn remover_deixa_lapide_e_persiste() {
    let p = caminho();
    let (mut c, _) = Cofre::criar(&p, DISP, SENHA, RAPIDO).unwrap();
    let id = c.adicionar(nova("site"), T0).unwrap();
    c.remover(id, T0 + 5).unwrap();
    assert!(c.entradas().is_empty());
    assert_eq!(
        c.remover(id, T0 + 6).unwrap_err(),
        ErroApi::EntradaNaoEncontrada
    );
    c.salvar().unwrap();
    drop(c);
    let r = Cofre::abrir(&p, DISP, &Credencial::Senha(SENHA)).unwrap();
    assert!(r.entradas().is_empty());
}

#[test]
fn trocar_senha_mestra_revoga_senha_e_recuperacao_antigas() {
    let p = caminho();
    let (mut c, rec_antiga) = Cofre::criar(&p, DISP, SENHA, RAPIDO).unwrap();
    c.adicionar(nova("site"), T0).unwrap();
    c.salvar().unwrap();
    assert_eq!(c.epoca_da_chave(), 0);
    assert_eq!(
        c.trocar_senha_mestra(b"", RAPIDO).unwrap_err(),
        ErroApi::SenhaVazia
    );
    let rec_nova = c
        .trocar_senha_mestra(b"nova senha ficticia", RAPIDO)
        .unwrap();
    assert_eq!(c.epoca_da_chave(), 1);
    c.salvar().unwrap();
    drop(c);

    assert!(Cofre::abrir(&p, DISP, &Credencial::Senha(SENHA)).is_err());
    let antiga = chave_de_recuperacao_para_texto(&rec_antiga);
    assert!(Cofre::abrir(&p, DISP, &Credencial::Recuperacao(&antiga)).is_err());
    let r = Cofre::abrir(&p, DISP, &Credencial::Senha(b"nova senha ficticia")).unwrap();
    assert_eq!(r.entradas().len(), 1);
    assert_eq!(r.epoca_da_chave(), 1);
    drop(r);
    let nova_txt = chave_de_recuperacao_para_texto(&rec_nova);
    let r = Cofre::abrir(&p, DISP, &Credencial::Recuperacao(&nova_txt)).unwrap();
    assert_eq!(r.entradas().len(), 1);
}

#[test]
fn chave_de_recuperacao_ida_e_volta_e_tolerancia() {
    let k = Chave::aleatoria().unwrap();
    let t = chave_de_recuperacao_para_texto(&k);
    assert_eq!(t.len(), 55 + 10); // 55 letras e 10 hífens
    assert!(t.split('-').all(|g| g.len() == 5));
    assert_eq!(
        chave_de_recuperacao_de_texto(&t).unwrap().como_bytes(),
        k.como_bytes()
    );
    let digitada = t.to_lowercase().replace('-', "  ");
    assert_eq!(
        chave_de_recuperacao_de_texto(&digitada)
            .unwrap()
            .como_bytes(),
        k.como_bytes()
    );
}

#[test]
fn chave_de_recuperacao_detecta_erro_de_digitacao() {
    let k = Chave::aleatoria().unwrap();
    let t = chave_de_recuperacao_para_texto(&k);
    // troca cada letra por outra e confere que todas são recusadas
    for i in (0..t.len()).filter(|&i| t.as_bytes()[i] != b'-') {
        let mut b = t.clone().into_bytes();
        b[i] = if b[i] == b'A' { b'B' } else { b'A' };
        let r = chave_de_recuperacao_de_texto(std::str::from_utf8(&b).unwrap());
        assert!(r.is_err(), "posição {i}");
    }
    assert!(chave_de_recuperacao_de_texto("").is_err());
    assert!(chave_de_recuperacao_de_texto(&t[..t.len() - 1]).is_err());
    assert!(chave_de_recuperacao_de_texto(&format!("{t}A")).is_err());
    assert!(chave_de_recuperacao_de_texto("1111-0000-!!!!").is_err());
}

#[test]
fn debug_do_cofre_nao_vaza_conteudo() {
    let p = caminho();
    let (mut c, _) = Cofre::criar(&p, DISP, SENHA, RAPIDO).unwrap();
    c.adicionar(nova("site"), T0).unwrap();
    let s = format!("{c:?} {:?}", c.entradas());
    assert!(!s.contains("senha-ficticia-1"));
}

#[test]
fn gerador_exposto_pela_api() {
    assert_eq!(gerar_senha(20, Classes::TODAS).unwrap().len(), 20);
    assert!(!VERSAO.is_empty());
}

#[test]
fn duas_instancias_nao_abrem_o_mesmo_cofre_ao_mesmo_tempo() {
    let p = caminho();
    let (c, _) = Cofre::criar(&p, DISP, SENHA, RAPIDO).unwrap();
    assert_eq!(
        Cofre::abrir(&p, DISP, &Credencial::Senha(SENHA)).unwrap_err(),
        ErroApi::CofreEmUso
    );
    drop(c);
    assert!(Cofre::abrir(&p, DISP, &Credencial::Senha(SENHA)).is_ok());
}

#[test]
fn texto_de_recuperacao_gigante_e_recusado() {
    let enorme = "A".repeat(1_000_000);
    assert!(chave_de_recuperacao_de_texto(&enorme).is_err());
}
