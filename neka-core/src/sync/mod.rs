//! Camada `sync`: mesclagem de duas versões do corpo (D11, a especificação do formato).
//!
//! Função pura, sem I/O: recebe dois corpos já abertos do mesmo cofre e
//! devolve o corpo mesclado. É comutativa, associativa e idempotente
//! (verificado por testes de propriedade em `tests/mesclagem.rs`).

mod mesclar;

pub use mesclar::{mesclar, podar_lapides, RETENCAO_LAPIDES_MS};
