//! Mede o tempo do Argon2id com os parâmetros padrão (calibração da D8).
//! Uso: `cargo run --release -p neka-core --example medir_kdf`
use neka_core::crypto::{chave_da_senha, ParametrosKdf, Sal};
use std::time::Instant;

fn main() {
    let p = ParametrosKdf::PADRAO;
    let sal = Sal([0; 16]);
    for _ in 0..3 {
        let t = Instant::now();
        let _ = chave_da_senha(b"senha de medicao", &sal, &p).expect("kdf");
        println!("{p:?}: {:?}", t.elapsed());
    }
}
