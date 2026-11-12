//! KeePass KDBX 4 (crate `keepass`). Na importação, o caminho do grupo vira
//! etiqueta e os campos personalizados viram campos extras; a lixeira é
//! ignorada. A exportação gera KDBX 4 com Argon2id nos parâmetros da D8.

use keepass::config::{DatabaseConfig, KdfConfig};
use keepass::db::{fields, Value};
use keepass::{Database, DatabaseKey};

use super::{ErroIo, Registro};

fn chave(senha: &str) -> DatabaseKey {
    DatabaseKey::new().with_password(senha)
}

/// Lê um KDBX (3.1 ou 4) protegido por senha.
pub fn importar_kdbx(bytes: &[u8], senha: &str) -> Result<Vec<Registro>, ErroIo> {
    let db = Database::parse(bytes, chave(senha)).map_err(|e| {
        use keepass::db::DatabaseOpenError as E;
        match e {
            E::UnsupportedVersion => ErroIo::KdbxNaoSuportado,
            _ => ErroIo::KdbxNaoAbriu,
        }
    })?;
    let lixeira = db.recycle_bin().map(|g| g.id());
    let mut saida = Vec::new();
    for e in db.iter_all_entries() {
        // grupo: caminho sem a raiz; pula o que estiver na lixeira
        let mut nomes = Vec::new();
        let mut na_lixeira = false;
        let mut g = Some(e.parent().id());
        while let Some(id) = g {
            let Some(atual) = db.group(id) else { break };
            if Some(id) == lixeira {
                na_lixeira = true;
            }
            g = atual.parent().map(|p| p.id());
            if g.is_some() {
                nomes.push(atual.name.clone());
            }
        }
        if na_lixeira {
            continue;
        }
        nomes.reverse();
        let mut r = Registro::default();
        r.title = e.get_title().unwrap_or_default().to_owned();
        r.username = e.get_username().unwrap_or_default().to_owned();
        r.password = e.get_password().unwrap_or_default().to_owned();
        r.url = e.get_url().unwrap_or_default().to_owned();
        r.notes = e.get(fields::NOTES).unwrap_or_default().to_owned();
        r.tags = e.tags.clone();
        let grupo = nomes.join("/");
        if !grupo.is_empty() {
            r.tags.push(grupo);
        }
        r.tags.retain(|t| !t.trim().is_empty());
        r.tags.sort();
        r.tags.dedup();
        for (k, v) in &e.fields {
            if !fields::KNOWN_FIELDS.contains(&k.as_str()) && !v.get().is_empty() {
                r.extra.insert(k.clone(), v.get().clone());
            }
        }
        if r.title.trim().is_empty() {
            r.title = if r.url.is_empty() {
                r.username.clone()
            } else {
                r.url.clone()
            };
        }
        if r.title.trim().is_empty() && r.password.is_empty() {
            continue;
        }
        saida.push(r);
    }
    Ok(saida)
}

/// Gera um KDBX 4 protegido por `senha`.
pub fn exportar_kdbx(registros: &[Registro], senha: &str) -> Result<Vec<u8>, ErroIo> {
    let mut config = DatabaseConfig::default();
    if let KdfConfig::Argon2 { version, .. } = config.kdf_config {
        config.kdf_config = KdfConfig::Argon2id {
            iterations: 3,
            memory: 256 * 1024 * 1024,
            parallelism: 4,
            version,
        };
    }
    let mut db = Database::with_config(config);
    db.meta.database_name = Some("neka".into());
    {
        let mut raiz = db.root_mut();
        raiz.name = "neka".into();
        for r in registros {
            let mut e = raiz.add_entry();
            e.set_unprotected(fields::TITLE, r.title.as_str());
            e.set_unprotected(fields::USERNAME, r.username.as_str());
            e.set(fields::PASSWORD, Value::protected(r.password.as_str()));
            e.set_unprotected(fields::URL, r.url.as_str());
            e.set_unprotected(fields::NOTES, r.notes.as_str());
            for (k, v) in &r.extra {
                e.set(k.as_str(), Value::protected(v.as_str()));
            }
            e.tags = r.tags.clone();
        }
    }
    let mut saida = Vec::new();
    db.save(&mut saida, chave(senha))
        .map_err(|_| ErroIo::KdbxNaoGravou)?;
    Ok(saida)
}
