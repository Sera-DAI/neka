//! Merge tests (D11): properties and concrete cases.

use neka_core::model::{Body, Entry, Tombstone, Uuid, MAX_HISTORY};
use neka_core::sync::{merge, prune_tombstones, TOMBSTONE_RETENTION_MS};
use proptest::prelude::*;

/// Few ids and devices, so versions collide a lot.
fn id(n: u8) -> Uuid {
    Uuid::from_bytes([n; 16])
}

/// (id, tombstone?, clock, date, device, password, history)
type Version = (u8, bool, u64, i64, u8, u8, Vec<(u8, i64)>);

fn version() -> impl Strategy<Value = Version> {
    (
        0u8..4,                                          // id
        any::<bool>(),                                   // tombstone?
        0u64..6,                                         // clock
        0i64..4,                                         // data
        0u8..3,                                          // device
        0u8..4,                                          // password
        prop::collection::vec((0u8..6, 0i64..20), 0..4), // history
    )
}

fn body() -> impl Strategy<Value = Body> {
    prop::collection::vec(version(), 0..8).prop_map(|vs| {
        let mut c = Body::default();
        let mut used = std::collections::HashSet::new();
        for (i, dead, clock, data, disp, password, hist) in vs {
            if !used.insert(i) {
                continue; // one id per body
            }
            c.device_clock = c.device_clock.max(clock);
            if dead {
                c.tombstones.push(Tombstone {
                    id: id(i),
                    deleted_at: data,
                    clock,
                    device_id: id(100 + disp),
                });
            } else {
                let mut e = Entry::new(id(i), id(100 + disp), format!("t{password}"), data, clock);
                e.password = format!("s{password}");
                for (h, t) in hist {
                    e.history.push(neka_core::model::OldPassword {
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
    fn commutative(a in body(), b in body()) {
        prop_assert_eq!(merge(&a, &b), merge(&b, &a));
    }

    /// Fully associative when no `id` has a tombstone.
    #[test]
    fn associative_without_tombstones(a in body(), b in body(), c in body()) {
        let (a, b, c) = (no_tombstones(a), no_tombstones(b), no_tombstones(c));
        prop_assert_eq!(merge(&merge(&a, &b), &c), merge(&a, &merge(&b, &c)));
    }

    /// With tombstones, it is associative in everything except the history of an entry
    /// that came back after being deleted (limitation stated in
    /// the format specification): who wins, the ids, the tombstones and the current passwords.
    #[test]
    fn associative_except_history_of_resurrected(a in body(), b in body(), c in body()) {
        prop_assert_eq!(
            no_history(merge(&merge(&a, &b), &c)),
            no_history(merge(&a, &merge(&b, &c)))
        );
    }

    /// Even in the case above, the devices converge: after each one
    /// merges with the other's result, all end up equal.
    #[test]
    fn converges(a in body(), b in body(), c in body()) {
        let x = merge(&merge(&a, &b), &c);
        let y = merge(&a, &merge(&b, &c));
        prop_assert_eq!(merge(&x, &y), merge(&y, &x));
        let z = merge(&x, &y);
        prop_assert_eq!(merge(&z, &x), z.clone());
        prop_assert_eq!(merge(&z, &y), z);
    }

    #[test]
    fn idempotent(a in body(), b in body()) {
        let m = merge(&a, &b);
        prop_assert_eq!(merge(&m, &m), m.clone());
        prop_assert_eq!(merge(&m, &b), m.clone());
        prop_assert_eq!(merge(&m, &a), m);
    }

    #[test]
    fn valid_result_without_loss_of_ids(a in body(), b in body()) {
        let m = merge(&a, &b);
        prop_assert!(m.validate().is_ok());
        let ids = |c: &Body| {
            let mut v: Vec<Uuid> = c.entries.iter().map(|e| e.id).chain(c.tombstones.iter().map(|l| l.id)).collect();
            v.sort();
            v.dedup();
            v
        };
        let mut expected = ids(&a);
        expected.extend(ids(&b));
        expected.sort();
        expected.dedup();
        prop_assert_eq!(ids(&m), expected);
        prop_assert_eq!(m.device_clock, a.device_clock.max(b.device_clock));
        for e in &m.entries {
            prop_assert!(e.history.len() <= MAX_HISTORY);
        }
    }
}

fn no_tombstones(mut c: Body) -> Body {
    c.tombstones.clear();
    c
}

fn no_history(mut c: Body) -> Body {
    for e in &mut c.entries {
        e.history.clear();
    }
    c
}

fn entry(n: u8, disp: u8, clock: u64, password: &str) -> Entry {
    let mut e = Entry::new(id(n), id(disp), "Exemplo".into(), clock as i64, clock);
    e.password = password.into();
    e
}

fn com(entries: Vec<Entry>, tombstones: Vec<Tombstone>) -> Body {
    let device_clock = entries
        .iter()
        .map(|e| e.clock)
        .chain(tombstones.iter().map(|l| l.clock))
        .max()
        .unwrap_or(0);
    Body {
        entries,
        tombstones,
        device_clock,
        ..Body::default()
    }
}

#[test]
fn concurrent_edit_keeps_the_losing_password_in_history() {
    let a = com(vec![entry(1, 10, 5, "password-a")], vec![]);
    let b = com(vec![entry(1, 11, 5, "password-b")], vec![]);
    let m = merge(&a, &b);
    assert_eq!(m.entries.len(), 1);
    let e = &m.entries[0];
    assert_eq!(e.password, "password-b", "desempate pelo device_id maior");
    assert!(e.history.iter().any(|h| h.password == "password-a"));
}

#[test]
fn newer_tombstone_deletes_and_newer_edit_resurrects() {
    let alive = com(vec![entry(1, 10, 3, "x")], vec![]);
    let tombstone = |clock| Tombstone {
        id: id(1),
        deleted_at: clock as i64,
        clock,
        device_id: id(11),
    };
    let m = merge(&alive, &com(vec![], vec![tombstone(4)]));
    assert!(m.entries.is_empty());
    assert_eq!(m.tombstones.len(), 1);
    let m = merge(&alive, &com(vec![], vec![tombstone(2)]));
    assert_eq!(m.entries.len(), 1);
    assert!(m.tombstones.is_empty());
}

#[test]
fn new_entries_from_both_sides_add_up() {
    let a = com(vec![entry(1, 10, 1, "a")], vec![]);
    let b = com(vec![entry(2, 11, 1, "b")], vec![]);
    assert_eq!(merge(&a, &b).entries.len(), 2);
}

#[test]
fn prune_only_removes_old_tombstones() {
    let mut c = com(
        vec![],
        vec![
            Tombstone {
                id: id(1),
                deleted_at: 0,
                clock: 1,
                device_id: id(10),
            },
            Tombstone {
                id: id(2),
                deleted_at: TOMBSTONE_RETENTION_MS,
                clock: 2,
                device_id: id(10),
            },
        ],
    );
    assert_eq!(prune_tombstones(&mut c, TOMBSTONE_RETENTION_MS + 1), 1);
    assert_eq!(c.tombstones[0].id, id(2));
}
