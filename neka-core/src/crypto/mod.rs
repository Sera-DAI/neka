//! Criptografia: KDF, cifra autenticada, embrulho de chaves, aleatoriedade
//! e gerador de senhas. Só primitivas RustCrypto (regra do projeto);
//! algoritmos e parâmetros conforme D8, D9 e a especificação do formato.

mod aead;
mod chave;
mod erro;
mod gerador;
mod kdf;
mod slot;

pub use aead::{abrir, selar, Nonce, TAM_ETIQUETA, TAM_NONCE};
pub use chave::{aleatorio, Chave, TAM_CHAVE};
pub use erro::ErroCripto;
pub use gerador::{gerar_senha, Classes, TAM_SENHA_MAX, TAM_SENHA_MIN};
pub use kdf::{chave_da_recuperacao, chave_da_senha, ParametrosKdf, Sal, TAM_SAL};
pub use slot::{desembrulhar, embrulhar, ChaveEmbrulhada, TipoSlot, TAM_EMBRULHADA};
