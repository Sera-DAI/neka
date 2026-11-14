use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{Entry, ModelError, Tombstone};

/// Version of the body schema this code understands.
pub const CURRENT_SCHEMA: u32 = 1;

/// Vault body (what gets encrypted), serialized as CBOR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Body {
    pub schema: u32,
    /// Highest logical clock seen in this vault.
    pub device_clock: u64,
    pub entries: Vec<Entry>,
    pub tombstones: Vec<Tombstone>,
}

impl Default for Body {
    fn default() -> Self {
        Self {
            schema: CURRENT_SCHEMA,
            device_clock: 0,
            entries: Vec::new(),
            tombstones: Vec::new(),
        }
    }
}

impl Body {
    /// Checks the schema, each entry and the uniqueness of the `id`s
    /// (an `id` cannot be an entry and a tombstone at the same time).
    pub fn validate(&self) -> Result<(), ModelError> {
        if self.schema != CURRENT_SCHEMA {
            return Err(ModelError::UnknownSchema);
        }
        let mut seen = HashSet::new();
        for e in &self.entries {
            e.validate()?;
            if !seen.insert(e.id) {
                return Err(ModelError::DuplicateId);
            }
        }
        for l in &self.tombstones {
            if !seen.insert(l.id) {
                return Err(ModelError::DuplicateId);
            }
        }
        Ok(())
    }

    /// Validates and serializes to CBOR.
    pub fn to_cbor(&self) -> Result<Vec<u8>, ModelError> {
        self.validate()?;
        let mut output = Vec::new();
        ciborium::into_writer(self, &mut output).map_err(|_| ModelError::CborNotSerializable)?;
        Ok(output)
    }

    /// Reads CBOR and validates. Any defect becomes a [`ModelError`] with fixed text.
    pub fn from_cbor(bytes: &[u8]) -> Result<Self, ModelError> {
        let body: Self = ciborium::from_reader(bytes).map_err(|_| ModelError::InvalidCbor)?;
        body.validate()?;
        Ok(body)
    }
}
