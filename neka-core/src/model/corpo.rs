use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use super::{Entrada, ErroModelo, Lapide};

/// Versão do esquema do corpo que este código entende.
pub const SCHEMA_ATUAL: u32 = 1;

/// Corpo do cofre (o que vai cifrado), serializado em CBOR.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Corpo {
    pub schema: u32,
    /// Maior relógio lógico visto neste cofre.
    pub device_clock: u64,
    pub entries: Vec<Entrada>,
    pub tombstones: Vec<Lapide>,
}

impl Default for Corpo {
    fn default() -> Self {
        Self {
            schema: SCHEMA_ATUAL,
            device_clock: 0,
            entries: Vec::new(),
            tombstones: Vec::new(),
        }
    }
}

impl Corpo {
    /// Confere o esquema, cada entrada e a unicidade dos `id`
    /// (um `id` não pode ser entrada e lápide ao mesmo tempo).
    pub fn validar(&self) -> Result<(), ErroModelo> {
        if self.schema != SCHEMA_ATUAL {
            return Err(ErroModelo::SchemaDesconhecido);
        }
        let mut vistos = HashSet::new();
        for e in &self.entries {
            e.validar()?;
            if !vistos.insert(e.id) {
                return Err(ErroModelo::IdDuplicado);
            }
        }
        for l in &self.tombstones {
            if !vistos.insert(l.id) {
                return Err(ErroModelo::IdDuplicado);
            }
        }
        Ok(())
    }

    /// Valida e serializa em CBOR.
    pub fn para_cbor(&self) -> Result<Vec<u8>, ErroModelo> {
        self.validar()?;
        let mut saida = Vec::new();
        ciborium::into_writer(self, &mut saida).map_err(|_| ErroModelo::CborNaoSerializavel)?;
        Ok(saida)
    }

    /// Lê CBOR e valida. Qualquer defeito vira [`ErroModelo`] de texto fixo.
    pub fn de_cbor(bytes: &[u8]) -> Result<Self, ErroModelo> {
        let corpo: Self = ciborium::from_reader(bytes).map_err(|_| ErroModelo::CborInvalido)?;
        corpo.validar()?;
        Ok(corpo)
    }
}
