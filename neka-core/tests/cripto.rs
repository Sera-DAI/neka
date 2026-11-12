//! Critérios de aceite da criptografia (etapa 3).

use neka_core::crypto::*;
use neka_core::model::Uuid;

/// Parâmetros mínimos aceitos, para os testes serem rápidos.
const RAPIDO: ParametrosKdf = ParametrosKdf {
    mem_kib: ParametrosKdf::MEM_MIN_KIB,
    iters: 1,
    lanes: 1,
};

fn cofre() -> Uuid {
    Uuid::from_u128(0x1234)
}

#[test]
fn senha_certa_abre_e_errada_falha() {
    let sal = Sal::aleatorio().unwrap();
    let dados = Chave::aleatoria().unwrap();
    let k = chave_da_senha(b"senha ficticia", &sal, &RAPIDO).unwrap();
    let e = embrulhar(&k, &dados, &cofre(), 0, TipoSlot::Senha).unwrap();

    let k2 = chave_da_senha(b"senha ficticia", &sal, &RAPIDO).unwrap();
    let aberta = desembrulhar(&k2, &e, &cofre(), 0, TipoSlot::Senha).unwrap();
    assert_eq!(aberta.como_bytes(), dados.como_bytes());

    let errada = chave_da_senha(b"senha ficticiA", &sal, &RAPIDO).unwrap();
    assert_eq!(
        desembrulhar(&errada, &e, &cofre(), 0, TipoSlot::Senha).unwrap_err(),
        ErroCripto::FalhaAutenticacao
    );
}

#[test]
fn sal_ou_parametros_diferentes_mudam_a_chave() {
    let s1 = Sal([1; 16]);
    let s2 = Sal([2; 16]);
    let a = chave_da_senha(b"x", &s1, &RAPIDO).unwrap();
    let b = chave_da_senha(b"x", &s2, &RAPIDO).unwrap();
    let c = chave_da_senha(b"x", &s1, &ParametrosKdf { iters: 2, ..RAPIDO }).unwrap();
    assert_ne!(a.como_bytes(), b.como_bytes());
    assert_ne!(a.como_bytes(), c.como_bytes());
}

#[test]
fn recuperacao_abre_o_proprio_slot() {
    let dados = Chave::aleatoria().unwrap();
    let rec = Chave::aleatoria().unwrap();
    let k = chave_da_recuperacao(&rec);
    let e = embrulhar(&k, &dados, &cofre(), 0, TipoSlot::Recuperacao).unwrap();
    let aberta = desembrulhar(
        &chave_da_recuperacao(&rec),
        &e,
        &cofre(),
        0,
        TipoSlot::Recuperacao,
    )
    .unwrap();
    assert_eq!(aberta.como_bytes(), dados.como_bytes());
    // a chave de embrulho não é a própria chave de recuperação
    assert_ne!(k.como_bytes(), rec.como_bytes());
}

#[test]
fn aad_amarra_cofre_e_tipo_do_slot() {
    let dados = Chave::aleatoria().unwrap();
    let k = Chave::aleatoria().unwrap();
    let e = embrulhar(&k, &dados, &cofre(), 0, TipoSlot::Senha).unwrap();
    assert!(desembrulhar(&k, &e, &Uuid::from_u128(0x9999), 0, TipoSlot::Senha).is_err());
    assert!(desembrulhar(&k, &e, &cofre(), 0, TipoSlot::Recuperacao).is_err());
    // a época da chave também entra no AAD (D15)
    assert!(desembrulhar(&k, &e, &cofre(), 1, TipoSlot::Senha).is_err());
}

#[test]
fn qualquer_byte_alterado_no_slot_falha() {
    let dados = Chave::aleatoria().unwrap();
    let k = Chave::aleatoria().unwrap();
    let e = embrulhar(&k, &dados, &cofre(), 0, TipoSlot::Senha).unwrap();
    for i in 0..TAM_EMBRULHADA {
        let mut m = e.clone();
        m.cifrada[i] ^= 0x01;
        assert!(
            desembrulhar(&k, &m, &cofre(), 0, TipoSlot::Senha).is_err(),
            "byte {i}"
        );
    }
    for i in 0..TAM_NONCE {
        let mut m = e.clone();
        m.nonce.0[i] ^= 0x01;
        assert!(
            desembrulhar(&k, &m, &cofre(), 0, TipoSlot::Senha).is_err(),
            "nonce {i}"
        );
    }
}

#[test]
fn selar_e_abrir_com_aad() {
    let k = Chave::aleatoria().unwrap();
    let n = Nonce::aleatorio().unwrap();
    let c = selar(&k, &n, b"cabecalho", b"corpo ficticio").unwrap();
    assert_eq!(c.len(), 14 + TAM_ETIQUETA);
    assert_eq!(abrir(&k, &n, b"cabecalho", &c).unwrap(), b"corpo ficticio");
    assert!(abrir(&k, &n, b"cabecalhO", &c).is_err());
    let mut t = c.clone();
    *t.last_mut().unwrap() ^= 1;
    assert!(abrir(&k, &n, b"cabecalho", &t).is_err());
    assert!(abrir(&k, &n, b"cabecalho", &c[..10]).is_err());
}

#[test]
fn nonces_e_chaves_aleatorias_nao_repetem() {
    let a = Nonce::aleatorio().unwrap();
    let b = Nonce::aleatorio().unwrap();
    assert_ne!(a, b);
    // embrulhar duas vezes a mesma chave dá nonces e textos diferentes
    let k = Chave::aleatoria().unwrap();
    let d = Chave::aleatoria().unwrap();
    let e1 = embrulhar(&k, &d, &cofre(), 0, TipoSlot::Senha).unwrap();
    let e2 = embrulhar(&k, &d, &cofre(), 0, TipoSlot::Senha).unwrap();
    assert_ne!(e1, e2);
}

#[test]
fn parametros_fora_dos_limites_sao_recusados() {
    let ruins = [
        ParametrosKdf {
            mem_kib: ParametrosKdf::MEM_MIN_KIB - 1,
            ..RAPIDO
        },
        ParametrosKdf {
            mem_kib: ParametrosKdf::MEM_MAX_KIB + 1,
            ..RAPIDO
        },
        ParametrosKdf {
            mem_kib: u32::MAX,
            ..RAPIDO
        },
        ParametrosKdf { iters: 0, ..RAPIDO },
        ParametrosKdf {
            iters: 17,
            ..RAPIDO
        },
        ParametrosKdf { lanes: 0, ..RAPIDO },
        ParametrosKdf {
            lanes: 17,
            ..RAPIDO
        },
    ];
    for p in ruins {
        assert_eq!(
            chave_da_senha(b"x", &Sal([0; 16]), &p).unwrap_err(),
            ErroCripto::ParametrosForaDosLimites,
            "{p:?}"
        );
    }
    assert!(ParametrosKdf::PADRAO.validar().is_ok());
}

#[test]
fn debug_e_erros_nao_vazam_chave() {
    let k = Chave::de_bytes([0xAB; 32]);
    let s = format!("{k:?}");
    assert!(
        !s.contains("171") && !s.to_lowercase().contains("ab, "),
        "{s}"
    );
    for e in [ErroCripto::FalhaAutenticacao, ErroCripto::FalhaKdf] {
        assert!(!e.to_string().is_empty());
    }
}

#[test]
fn gerador_respeita_tamanho_e_classes() {
    for _ in 0..200 {
        let s = gerar_senha(16, Classes::TODAS).unwrap();
        assert_eq!(s.len(), 16);
        assert!(s.bytes().any(|b| b.is_ascii_lowercase()));
        assert!(s.bytes().any(|b| b.is_ascii_uppercase()));
        assert!(s.bytes().any(|b| b.is_ascii_digit()));
        assert!(s.bytes().any(|b| b.is_ascii_punctuation()));
    }
    let so_digitos = Classes {
        minusculas: false,
        maiusculas: false,
        digitos: true,
        simbolos: false,
    };
    assert!(gerar_senha(20, so_digitos)
        .unwrap()
        .bytes()
        .all(|b| b.is_ascii_digit()));
    assert_ne!(
        gerar_senha(32, Classes::TODAS).unwrap(),
        gerar_senha(32, Classes::TODAS).unwrap()
    );
}

#[test]
fn gerador_recusa_pedidos_impossiveis() {
    let nada = Classes {
        minusculas: false,
        maiusculas: false,
        digitos: false,
        simbolos: false,
    };
    assert_eq!(
        gerar_senha(16, nada).unwrap_err(),
        ErroCripto::GeradorInvalido
    );
    assert!(gerar_senha(TAM_SENHA_MIN - 1, Classes::TODAS).is_err());
    assert!(gerar_senha(TAM_SENHA_MAX + 1, Classes::TODAS).is_err());
}

#[test]
fn gerador_distribui_sem_vies_grosseiro() {
    // 20 mil dígitos: cada um deve sair perto de 10% (tolerância folgada).
    let so_digitos = Classes {
        minusculas: false,
        maiusculas: false,
        digitos: true,
        simbolos: false,
    };
    let mut cont = [0u32; 10];
    for _ in 0..100 {
        for b in gerar_senha(200, so_digitos).unwrap().bytes() {
            cont[(b - b'0') as usize] += 1;
        }
    }
    for c in cont {
        assert!((1700..2300).contains(&c), "{cont:?}");
    }
}
