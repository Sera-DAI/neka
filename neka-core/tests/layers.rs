//! Enforces the dependency rule between layers of the project.
//! A layer may only reference (`crate::x`, `super::x`, `neka_core::x`)
//! the layers listed in ALLOWED. The interface (`neka-cli`) may only use `neka_core::api`.

use std::fs;
use std::path::{Path, PathBuf};

const LAYERS: [&str; 6] = ["model", "crypto", "store", "sync", "io", "api"];

fn allowed(layer: &str) -> &'static [&'static str] {
    match layer {
        "model" => &[],
        "crypto" | "store" => &["model", "crypto", "store"],
        "sync" | "io" => &["model", "crypto", "store", "sync", "io"],
        "api" => &["model", "crypto", "store", "sync", "io", "api"],
        _ => unreachable!(),
    }
}

fn rs_files(dir: &Path, out: &mut Vec<PathBuf>) {
    for e in fs::read_dir(dir).unwrap() {
        let p = e.unwrap().path();
        if p.is_dir() {
            rs_files(&p, out);
        } else if p.extension().is_some_and(|x| x == "rs") {
            out.push(p);
        }
    }
}

/// Layers named in the text after the prefix (e.g. `crate::`).
fn quoted(text: &str, prefix: &str) -> Vec<String> {
    text.match_indices(prefix)
        .map(|(i, _)| {
            let rest = &text[i + prefix.len()..];
            let rest = rest.trim_start_matches(['{', ' ']);
            rest.chars()
                .take_while(|c| c.is_alphanumeric() || *c == '_')
                .collect()
        })
        .collect()
}

/// Returns the violations found: "file: depends on x".
pub fn core_violations(root: &Path) -> Vec<String> {
    let mut v = Vec::new();
    for layer in LAYERS {
        let mut files = Vec::new();
        rs_files(&root.join("src").join(layer), &mut files);
        for file in files {
            let text = fs::read_to_string(&file).unwrap();
            for prefix in ["crate::", "neka_core::"] {
                for target in quoted(&text, prefix) {
                    if LAYERS.contains(&target.as_str())
                        && !allowed(layer).contains(&target.as_str())
                    {
                        v.push(format!("{}: {layer} depende de {target}", file.display()));
                    }
                }
            }
            for target in quoted(&text, "super::super::") {
                if LAYERS.contains(&target.as_str()) && !allowed(layer).contains(&target.as_str()) {
                    v.push(format!("{}: {layer} depende de {target}", file.display()));
                }
            }
        }
    }
    v
}

pub fn cli_violations(root: &Path) -> Vec<String> {
    let mut v = Vec::new();
    let mut files = Vec::new();
    rs_files(&root.join("../neka-cli/src"), &mut files);
    for file in files {
        let text = fs::read_to_string(&file).unwrap();
        for target in quoted(&text, "neka_core::") {
            if target != "api" {
                v.push(format!(
                    "{}: interface usa neka_core::{target}",
                    file.display()
                ));
            }
        }
    }
    v
}

#[test]
fn core_layers_follow_the_rule() {
    let v = core_violations(Path::new(env!("CARGO_MANIFEST_DIR")));
    assert!(v.is_empty(), "violations:\n{}", v.join("\n"));
}

#[test]
fn interface_only_uses_the_api() {
    let v = cli_violations(Path::new(env!("CARGO_MANIFEST_DIR")));
    assert!(v.is_empty(), "violations:\n{}", v.join("\n"));
}

#[test]
fn checker_detects_violation() {
    // Creates a temporary tree that breaks the rule on purpose.
    let t = std::env::temp_dir().join(format!("neka-layers-{}", std::process::id()));
    let _ = fs::remove_dir_all(&t);
    fs::create_dir_all(t.join("src/model")).unwrap();
    fs::create_dir_all(t.join("../neka-cli-fake")).ok();
    fs::write(t.join("src/model/mod.rs"), "use crate::crypto::Algo;\n").unwrap();
    for c in LAYERS.iter().filter(|c| **c != "model") {
        fs::create_dir_all(t.join("src").join(c)).unwrap();
    }
    let v = core_violations(&t);
    let _ = fs::remove_dir_all(&t);
    assert_eq!(v.len(), 1, "{v:?}");
    assert!(v[0].contains("model depende de crypto"));
}
