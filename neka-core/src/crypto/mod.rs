//! Cryptography: KDF, authenticated cipher, key wrapping, randomness
//! and password generator. Only RustCrypto primitives (project rule);
//! algorithms and params as in D8, D9 and the format specification.

mod aead;
mod error;
mod generator;
mod kdf;
mod key;
mod slot;

pub use aead::{open, seal, Nonce, NONCE_LEN, TAG_LEN};
pub use error::CryptoError;
pub use generator::{generate_password, Classes, PASSWORD_LEN_MAX, PASSWORD_LEN_MIN};
pub use kdf::{password_key, recovery_key, KdfParams, Salt, SALT_LEN};
pub use key::{random, Key, KEY_LEN};
pub use slot::{unwrap, wrap, SlotKind, WrappedKey, WRAPPED_LEN};
