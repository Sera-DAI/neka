//! Domínio: tipos do cofre, validações e serialização CBOR. Sem I/O.
//!
//! Tempo (`now`), identificadores e relógio lógico entram sempre por
//! parâmetro: esta camada não lê relógio nem sorteia nada.

mod corpo;
mod entrada;
mod erro;

pub use corpo::{Corpo, SCHEMA_ATUAL};
pub use entrada::{Entrada, Lapide, SenhaAntiga, MAX_HISTORICO};
pub use erro::ErroModelo;
pub use uuid::Uuid;
