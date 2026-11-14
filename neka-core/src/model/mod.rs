//! Domain: vault types, validation and CBOR serialization. No I/O.
//!
//! Time (`now`), identifiers and the logical clock always come in as
//! parameters: this layer neither reads a clock nor draws anything random.

mod body;
mod entry;
mod error;

pub use body::{Body, CURRENT_SCHEMA};
pub use entry::{Entry, OldPassword, Tombstone, MAX_HISTORY};
pub use error::ModelError;
pub use uuid::Uuid;
