//! Measures the Argon2id time with the default params (calibration of D8).
//! Usage: `cargo run --release -p neka-core --example measure_kdf`
use neka_core::crypto::{password_key, KdfParams, Salt};
use std::time::Instant;

fn main() {
    let p = KdfParams::DEFAULT;
    let salt = Salt([0; 16]);
    for _ in 0..3 {
        let t = Instant::now();
        let _ = password_key(b"password de measurement", &salt, &p).expect("kdf");
        println!("{p:?}: {:?}", t.elapsed());
    }
}
