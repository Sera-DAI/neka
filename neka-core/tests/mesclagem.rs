//! Testes da mesclagem (D11): propriedades e casos concretos.

use neka_core::model::{Corpo, Entrada, Lapide, Uuid, MAX_HISTORICO};
use neka_core::sync::{mesclar, podar_lapides, RETENCAO_LAPIDES_MS};
use proptest::prelude::*;

/// Poucos ids e dispositivos, para as versões colidirem bastante.
fn id(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}

/// (id, lápide?, clock, data, dispositivo, senha, histórico)
type Versao = (u8, bool, u64, i64, u8, u8, Vec<(u8, i64)>);

fn versao() -> impl Strategy<Value = Versao> {
    (
        0u8..4,                                          // id
        any::<bool>(),                                   // lápide?
        0u64..6,                                         // clock
        0i64..4,                                         // data
        0u8..3,                                          // dispositivo
        0u8..4,                                          // senha
        prop::collection::vec((0u8..6, 0i64..20), 0..4), // histórico
    )
}

fn corpo() -> impl Strategy<Value = Corpo> {
    prop::collection::vec(versao(), 0..8).prop_map(|vs| {
        let mut c = Corpo::default();
        let mut usados = std::collections::HashSet::new();
        for (i, morta, clock, data, disp, senha, hist) in vs {
            if !usados.insert(i) {
                continue; // um id por corpo
            }
            c.device_clock = c.device_clock.max(clock);
            if morta {
                c.tombstones.push(Lapide {
                    id: id(i),
                    deleted_at: data,
                    clock,
                    device_id: id(100 + disp),
                });
            } else {
                let mut e = Entrada::nova(id(i), id(100 + disp), format!("t{senha}"), data, clock);
                e.password = format!("s{senha}");
                for (h, t) in hist {
                    e.history.push(neka_core::model::SenhaAntiga {
                        password: format!("s{h}"),
                        changed_at: t,
                    });
                }
                c.entries.push(e);
            }
        }
        c
    })
}

proptest! {
    #![proptest_config(ProptestConfig::with_cases(2000))]

    #[test]
    fn comutativa(a in corpo(), b in corpo()) {
        prop_assert_eq!(mesclar(&a, &b), mesclar(&b, &a));
    }

    /// Associativa por inteiro quando nenhum `id` tem lápide.
    #[test]
    fn associativa_sem_lapides(a in corpo(), b in corpo(), c in corpo()) {
        let (a, b, c) = (sem_lapides(a), sem_lapides(b), sem_lapides(c));
        prop_assert_eq!(mesclar(&mesclar(&a, &b), &c), mesclar(&a, &mesclar(&b, &c)));
    }

    /// Com lápides, é associativa em tudo menos no histórico de uma entrada
    /// que voltou depois de excluída (limitação declarada em
    /// a especificação do formato): quem vence, os ids, as lápides e as senhas atuais.
    #[test]
    fn associativa_exceto_historico_de_ressuscitadas(a in corpo(), b in corpo(), c in corpo()) {
        prop_assert_eq!(
            sem_historico(mesclar(&mesclar(&a, &b), &c)),
            sem_historico(mesclar(&a, &mesclar(&b, &c)))
        );
    }

    /// Mesmo no caso acima, os dispositivos convergem: depois de cada um
    /// mesclar com o resultado do outro, todos ficam iguais.
    #[test]
    fn converge(a in corpo(), b in corpo(), c in corpo()) {
        let x = mesclar(&mesclar(&a, &b), &c);
        let y = mesclar(&a, &mesclar(&b, &c));
        prop_assert_eq!(mesclar(&x, &y), mesclar(&y, &x));
        let z = mesclar(&x, &y);
        prop_assert_eq!(mesclar(&z, &x), z.clone());
        prop_assert_eq!(mesclar(&z, &y), z);
    }

    #[test]
    fn idempotente(a in corpo(), b in corpo()) {
        let m = mesclar(&a, &b);
        prop_assert_eq!(mesclar(&m, &m), m.clone());
        prop_assert_eq!(mesclar(&m, &b), m.clone());
        prop_assert_eq!(mesclar(&m, &a), m);
    }

    #[test]
    fn resultado_valido_e_sem_perda_de_ids(a in corpo(), b in corpo()) {
        let m = mesclar(&a, &b);
        prop_assert!(m.validar().is_ok());
        let ids = |c: &Corpo| {
            let mut v: Vec<Uuid> = c.entries.iter().map(|e| e.id).chain(c.tombstones.iter().map(|l| l.id)).collect();
            v.sort();
            v.dedup();
            v
        };
        let mut esperado = ids(&a);
        esperado.extend(ids(&b));
        esperado.sort();
        esperado.dedup();
        prop_assert_eq!(ids(&m), esperado);
        prop_assert_eq!(m.device_clock, a.device_clock.max(b.device_clock));
        for e in &m.entries {
            prop_assert!(e.history.len() <= MAX_HISTORICO);
        }
    }
}

fn sem_lapides(mut c: Corpo) -> Corpo {
    c.tombstones.clear();
    c
}

fn sem_historico(mut c: Corpo) -> Corpo {
    for e in &mut c.entries {
        e.history.clear();
    }
    c
}

fn entrada(n: u8, disp: u8, clock: u64, senha: &str) -> Entrada {
    let mut e = Entrada::nova(id(n), id(disp), "Exemplo".into(), clock as i64, clock);
    e.password = senha.into();
    e
}

fn com(entries: Vec<Entrada>, tombstones: Vec<Lapide>) -> Corpo {
    let device_clock = entries
        .iter()
        .map(|e| e.clock)
        .chain(tombstones.iter().map(|l| l.clock))
        .max()
        .unwrap_or(0);
    Corpo {
        entries,
        tombstones,
        device_clock,
        ..Corpo::default()
    }
}

#[test]
fn edicao_concorrente_guarda_a_senha_perdedora_no_historico() {
    let a = com(vec![entrada(1, 10, 5, "senha-a")], vec![]);
    let b = com(vec![entrada(1, 11, 5, "senha-b")], vec![]);
    let m = mesclar(&a, &b);
    assert_eq!(m.entries.len(), 1);
    let e = &m.entries[0];
    assert_eq!(e.password, "senha-b", "desempate pelo device_id maior");
    assert!(e.history.iter().any(|h| h.password == "senha-a"));
}

#[test]
fn lapide_mais_nova_apaga_e_edicao_mais_nova_ressuscita() {
    let viva = com(vec![entrada(1, 10, 3, "x")], vec![]);
    let lapide = |clock| Lapide {
        id: id(1),
        deleted_at: clock as i64,
        clock,
        device_id: id(11),
    };
    let m = mesclar(&viva, &com(vec![], vec![lapide(4)]));
    assert!(m.entries.is_empty());
    assert_eq!(m.tombstones.len(), 1);
    let m = mesclar(&viva, &com(vec![], vec![lapide(2)]));
    assert_eq!(m.entries.len(), 1);
    assert!(m.tombstones.is_empty());
}

#[test]
fn entradas_novas_dos_dois_lados_somam() {
    let a = com(vec![entrada(1, 10, 1, "a")], vec![]);
    let b = com(vec![entrada(2, 11, 1, "b")], vec![]);
    assert_eq!(mesclar(&a, &b).entries.len(), 2);
}

#[test]
fn poda_so_remove_lapides_velhas() {
    let mut c = com(
        vec![],
        vec![
            Lapide {
                id: id(1),
                deleted_at: 0,
                clock: 1,
                device_id: id(10),
            },
            Lapide {
                id: id(2),
                deleted_at: RETENCAO_LAPIDES_MS,
                clock: 2,
                device_id: id(10),
            },
        ],
    );
    assert_eq!(podar_lapides(&mut c, RETENCAO_LAPIDES_MS + 1), 1);
    assert_eq!(c.tombstones[0].id, id(2));
}
