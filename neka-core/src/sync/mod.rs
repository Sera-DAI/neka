//! `sync` layer: merge of two versions of the body (D11, the format specification).
//!
//! Pure function, no I/O: takes two already-open bodies of the same vault and
//! returns the merged body. It is commutative, associative and idempotent
//! (verified by property tests in `tests/merging.rs`).

mod merge;

pub use merge::{merge, prune_tombstones, TOMBSTONE_RETENTION_MS};
