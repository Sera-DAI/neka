//! Impõe a regra de dependência entre camadas do projeto.
//! Uma camada só pode referenciar (`crate::x`, `super::x`, `neka_core::x`)
//! as camadas listadas em PERMITIDAS. A interface (`neka-cli`) só pode usar `neka_core::api`.

use std::fs;
use std::path::{Path, PathBuf};

const CAMADAS: [&str; 6] = ["model", "crypto", "store", "sync", "io", "api"];

fn permitidas(camada: &str) -> &'static [&'static str] {
    match camada {
        "model" => &[],
        "crypto" | "store" => &["model", "crypto", "store"],
        "sync" | "io" => &["model", "crypto", "store", "sync", "io"],
        "api" => &["model", "crypto", "store", "sync", "io", "api"],
        _ => unreachable!(),
    }
}

fn arquivos_rs(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            arquivos_rs(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Camadas citadas no texto após o prefixo (ex.: `crate::`).
fn citadas(texto: &str, prefixo: &str) -> Vec<String> {
    texto
        .match_indices(prefixo)
        .map(|(i, _)| {
            let resto = &texto[i + prefixo.len()..];
            let resto = resto.trim_start_matches(['{', ' ']);
            resto
                .chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect()
        })
        .collect()
}

/// Devolve as violações encontradas: "arquivo: depende de x".
pub fn violacoes_core(raiz: &Path) -> Vec<String> {
    let mut v = Vec::new();
    for camada in CAMADAS {
        let mut arqs = Vec::new();
        arquivos_rs(&raiz.join("src").join(camada), &mut arqs);
        for arq in arqs {
            let texto = fs::read_to_string(&arq).unwrap();
            for prefixo in ["crate::", "neka_core::"] {
                for alvo in citadas(&texto, prefixo) {
                    if CAMADAS.contains(&alvo.as_str())
                        && !permitidas(camada).contains(&alvo.as_str())
                    {
                        v.push(format!("{}: {camada} depende de {alvo}", arq.display()));
                    }
                }
            }
            for alvo in citadas(&texto, "super::super::") {
                if CAMADAS.contains(&alvo.as_str()) && !permitidas(camada).contains(&alvo.as_str())
                {
                    v.push(format!("{}: {camada} depende de {alvo}", arq.display()));
                }
            }
        }
    }
    v
}

pub fn violacoes_cli(raiz: &Path) -> Vec<String> {
    let mut v = Vec::new();
    let mut arqs = Vec::new();
    arquivos_rs(&raiz.join("../neka-cli/src"), &mut arqs);
    for arq in arqs {
        let texto = fs::read_to_string(&arq).unwrap();
        for alvo in citadas(&texto, "neka_core::") {
            if alvo != "api" {
                v.push(format!(
                    "{}: interface usa neka_core::{alvo}",
                    arq.display()
                ));
            }
        }
    }
    v
}

#[test]
fn camadas_do_core_respeitam_a_regra() {
    let v = violacoes_core(Path::new(env!("CARGO_MANIFEST_DIR")));
    assert!(v.is_empty(), "violações:\n{}", v.join("\n"));
}

#[test]
fn interface_so_usa_a_api() {
    let v = violacoes_cli(Path::new(env!("CARGO_MANIFEST_DIR")));
    assert!(v.is_empty(), "violações:\n{}", v.join("\n"));
}

#[test]
fn verificador_detecta_violacao() {
    // Cria uma árvore temporária que quebra a regra de propósito.
    let t = std::env::temp_dir().join(format!("neka-camadas-{}", std::process::id()));
    let _ = fs::remove_dir_all(&t);
    fs::create_dir_all(t.join("src/model")).unwrap();
    fs::create_dir_all(t.join("../neka-cli-fake")).ok();
    fs::write(t.join("src/model/mod.rs"), "use crate::crypto::Algo;\n").unwrap();
    for c in CAMADAS.iter().filter(|c| **c != "model") {
        fs::create_dir_all(t.join("src").join(c)).unwrap();
    }
    let v = violacoes_core(&t);
    let _ = fs::remove_dir_all(&t);
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(v[0].contains("model depende de crypto"));
}
