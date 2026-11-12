//! Critérios de aceite do domínio (etapa 2).

use neka_core::model::{Corpo, Entrada, ErroModelo, Lapide, Uuid, MAX_HISTORICO};

fn id(n: u128) -> Uuid {
    Uuid::from_u128(n)
}

fn entrada(n: u128) -> Entrada {
    let mut e = Entrada::nova(id(n), id(900), format!("site {n}"), 1_700_000_000_000, 1);
    e.username = "ana".into();
    e.password = "senha-ficticia".into();
    e.url = "https://exemplo.invalid".into();
    e.notes = "nota ficticia".into();
    e.tags = vec!["banco".into()];
    e.extra
        .insert("pergunta".into(), "resposta ficticia".into());
    e
}

fn corpo() -> Corpo {
    let mut c = Corpo::default();
    c.entries.push(entrada(1));
    c.entries.push(entrada(2));
    c.tombstones.push(Lapide {
        id: id(3),
        deleted_at: 1_700_000_001_000,
        clock: 5,
        device_id: id(900),
    });
    c.device_clock = 5;
    c
}

#[test]
fn cbor_ida_e_volta_preserva_tudo() {
    let c = corpo();
    let bytes = c.para_cbor().unwrap();
    assert_eq!(Corpo::de_cbor(&bytes).unwrap(), c);
}

#[test]
fn cbor_e_deterministico() {
    assert_eq!(corpo().para_cbor().unwrap(), corpo().para_cbor().unwrap());
}

#[test]
fn cbor_lixo_e_truncado_sao_recusados() {
    assert_eq!(
        Corpo::de_cbor(b"nao e cbor").unwrap_err(),
        ErroModelo::CborInvalido
    );
    let bytes = corpo().para_cbor().unwrap();
    assert!(Corpo::de_cbor(&bytes[..bytes.len() / 2]).is_err());
    assert!(Corpo::de_cbor(&[]).is_err());
}

#[test]
fn schema_desconhecido_e_recusado() {
    let mut c = corpo();
    c.schema = 2;
    assert_eq!(c.validar().unwrap_err(), ErroModelo::SchemaDesconhecido);
    assert!(c.para_cbor().is_err());
}

#[test]
fn id_repetido_e_recusado() {
    let mut c = corpo();
    c.entries.push(entrada(1));
    assert_eq!(c.validar().unwrap_err(), ErroModelo::IdDuplicado);
    // entrada e lápide com o mesmo id também
    let mut c = corpo();
    c.tombstones[0].id = id(1);
    assert_eq!(c.validar().unwrap_err(), ErroModelo::IdDuplicado);
}

#[test]
fn limites_de_campos() {
    let mut e = entrada(1);
    e.title = "  ".into();
    assert_eq!(e.validar().unwrap_err(), ErroModelo::TituloVazio);
    let mut e = entrada(1);
    e.title = "x".repeat(257);
    assert_eq!(e.validar().unwrap_err(), ErroModelo::TituloLongo);
    let mut e = entrada(1);
    e.password = "x".repeat(4097);
    assert_eq!(e.validar().unwrap_err(), ErroModelo::SenhaLonga);
    let mut e = entrada(1);
    e.tags = vec![String::new()];
    assert_eq!(e.validar().unwrap_err(), ErroModelo::EtiquetaInvalida);
    let mut e = entrada(1);
    e.extra.insert(String::new(), "v".into());
    assert_eq!(e.validar().unwrap_err(), ErroModelo::ExtraInvalido);
    assert!(entrada(1).validar().is_ok());
}

#[test]
fn historico_guarda_no_maximo_dez() {
    let mut e = entrada(1);
    for i in 0..(MAX_HISTORICO + 5) {
        e.trocar_senha(format!("senha-{i}"), 1_700_000_100_000 + i as i64);
    }
    assert_eq!(e.history.len(), MAX_HISTORICO);
    // a mais antiga saiu primeiro; a primeira senha original já não está
    assert!(e.history.iter().all(|h| h.password != "senha-ficticia"));
    assert_eq!(
        e.history.last().unwrap().password,
        format!("senha-{}", MAX_HISTORICO + 3)
    );
    assert!(e.validar().is_ok());
}

#[test]
fn debug_nao_vaza_segredos() {
    let mut e = entrada(1);
    e.trocar_senha("outra-senha-ficticia".into(), 1_700_000_100_000);
    let saida = format!("{e:?} {:?}", corpo());
    for segredo in [
        "senha-ficticia",
        "nota ficticia",
        "resposta ficticia",
        "outra-senha-ficticia",
    ] {
        assert!(!saida.contains(segredo), "vazou: {segredo}");
    }
}

#[test]
fn erros_tem_texto_fixo_sem_conteudo() {
    let mut e = entrada(1);
    e.password = "SEGREDO".repeat(1000);
    let msg = e.validar().unwrap_err().to_string();
    assert!(!msg.contains("SEGREDO"));
}
