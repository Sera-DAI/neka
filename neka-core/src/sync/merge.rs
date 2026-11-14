use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use crate::model::{Body, Entry, OldPassword, Tombstone, Uuid, MAX_HISTORY};

/// Tombstones older than this can be removed (D12: 180 days).
pub const TOMBSTONE_RETENTION_MS: i64 = 180 * 24 * 60 * 60 * 1000;

/// A version of an `id`: live entry or tombstone.
#[derive(Clone, Copy)]
enum Version<'a> {
    Alive(&'a Entry),
    Dead(&'a Tombstone),
}

impl Version<'_> {
    /// D11 tuple: `(clock, updated_at or deleted_at, device_id)`.
    fn key(&self) -> (u64, i64, Uuid) {
        match self {
            Self::Alive(e) => (e.clock, e.updated_at, e.device_id),
            Self::Dead(l) => (l.clock, l.deleted_at, l.device_id),
        }
    }

    /// Total order: the D11 tuple and, only to break ties between versions that
    /// diverge with the same tuple (does not happen with an honest device),
    /// tombstone before entry and then the serialized content.
    fn compare(&self, other: &Self) -> Ordering {
        self.key()
            .cmp(&other.key())
            .then_with(|| match (self, other) {
                (Self::Dead(a), Self::Dead(b)) => a.deleted_at.cmp(&b.deleted_at),
                (Self::Dead(_), Self::Alive(_)) => Ordering::Greater,
                (Self::Alive(_), Self::Dead(_)) => Ordering::Less,
                (Self::Alive(a), Self::Alive(b)) => bytes(a).cmp(&bytes(b)),
            })
    }
}

fn bytes(e: &Entry) -> Vec<u8> {
    let mut v = Vec::new();
    // Serializing a valid entry does not fail; if it does, empty wins.
    let _ = ciborium::into_writer(e, &mut v);
    v
}

/// Merges histories: keeps each password once (with the most recent instant
/// it was changed) and keeps the [`MAX_HISTORY`] most recent, from the
/// oldest to the newest.
fn merge_history<'a>(items: impl Iterator<Item = (&'a str, i64)>) -> Vec<OldPassword> {
    let mut by_password: HashMap<&str, i64> = HashMap::new();
    for (s, t) in items {
        let current = by_password.entry(s).or_insert(t);
        if t > *current {
            *current = t;
        }
    }
    let mut v: Vec<(i64, &str)> = by_password.into_iter().map(|(s, t)| (t, s)).collect();
    v.sort_unstable();
    let cut = v.len().saturating_sub(MAX_HISTORY);
    v.drain(cut..)
        .map(|(t, s)| OldPassword {
            password: s.to_owned(),
            changed_at: t,
        })
        .collect()
}

/// Merges two bodies of the same vault, entry by entry (D11).
///
/// For each `id`, the version with the highest `(clock, date, device_id)` wins. If the
/// winner is an entry, the passwords of all live versions go into its
/// history (nothing is lost; the current one is recorded too). If it is a tombstone, the entry disappears.
/// The result is sorted by `id`, so it does not depend on the input order.
pub fn merge(a: &Body, b: &Body) -> Body {
    let mut versions: BTreeMap<Uuid, Vec<Version<'_>>> = BTreeMap::new();
    for c in [a, b] {
        for e in &c.entries {
            versions.entry(e.id).or_default().push(Version::Alive(e));
        }
        for l in &c.tombstones {
            versions.entry(l.id).or_default().push(Version::Dead(l));
        }
    }

    let mut entries = Vec::new();
    let mut tombstones = Vec::new();
    for (_, vs) in versions {
        let Some(venc) = vs.iter().copied().max_by(|x, y| x.compare(y)) else {
            continue;
        };
        match venc {
            Version::Dead(l) => tombstones.push(l.clone()),
            Version::Alive(e) => {
                let mut new = e.clone();
                // All live versions go into the history, including the
                // winner: that way the result only depends on the set of
                // versions, and the merge stays associative. The interface
                // hides from the history the password equal to the current one.
                let mut items: Vec<(&str, i64)> = Vec::new();
                for v in &vs {
                    if let Version::Alive(x) = v {
                        items.extend(
                            x.history
                                .iter()
                                .map(|h| (h.password.as_str(), h.changed_at)),
                        );
                        if !x.password.is_empty() {
                            items.push((x.password.as_str(), x.updated_at));
                        }
                    }
                }
                new.history = merge_history(items.into_iter());
                entries.push(new);
            }
        }
    }

    Body {
        schema: a.schema.max(b.schema),
        device_clock: a.device_clock.max(b.device_clock),
        entries,
        tombstones,
    }
}

/// Removes tombstones older than [`TOMBSTONE_RETENTION_MS`] (D12). Returns
/// how many left. Kept out of [`merge`] so the merge stays pure.
pub fn prune_tombstones(body: &mut Body, now: i64) -> usize {
    let before = body.tombstones.len();
    body.tombstones
        .retain(|l| now.saturating_sub(l.deleted_at) <= TOMBSTONE_RETENTION_MS);
    before - body.tombstones.len()
}
