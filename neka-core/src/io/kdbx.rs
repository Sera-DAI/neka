//! KeePass KDBX 4 (crate `keepass`). On import, the group path becomes a
//! tag and custom fields become extra fields; the recycle bin is
//! ignored. Export generates KDBX 4 with Argon2id using D8's params.

use keepass::config::{DatabaseConfig, KdfConfig};
use keepass::db::{fields, Value};
use keepass::{Database, DatabaseKey};

use super::{IoError, Record};

fn key(password: &str) -> DatabaseKey {
    DatabaseKey::new().with_password(password)
}

/// Reads a KDBX (3.1 or 4) protected by a password.
pub fn import_kdbx(bytes: &[u8], password: &str) -> Result<Vec<Record>, IoError> {
    let db = Database::parse(bytes, key(password)).map_err(|e| {
        use keepass::db::DatabaseOpenError as E;
        match e {
            E::UnsupportedVersion => IoError::KdbxUnsupported,
            _ => IoError::KdbxOpenFailed,
        }
    })?;
    let recycle_bin = db.recycle_bin().map(|g| g.id());
    let mut output = Vec::new();
    for e in db.iter_all_entries() {
        // group: path without the root; skips what is in the recycle bin
        let mut names = Vec::new();
        let mut in_recycle_bin = false;
        let mut g = Some(e.parent().id());
        while let Some(id) = g {
            let Some(current) = db.group(id) else { break };
            if Some(id) == recycle_bin {
                in_recycle_bin = true;
            }
            g = current.parent().map(|p| p.id());
            if g.is_some() {
                names.push(current.name.clone());
            }
        }
        if in_recycle_bin {
            continue;
        }
        names.reverse();
        let mut r = Record::default();
        r.title = e.get_title().unwrap_or_default().to_owned();
        r.username = e.get_username().unwrap_or_default().to_owned();
        r.password = e.get_password().unwrap_or_default().to_owned();
        r.url = e.get_url().unwrap_or_default().to_owned();
        r.notes = e.get(fields::NOTES).unwrap_or_default().to_owned();
        r.tags = e.tags.clone();
        let group = names.join("/");
        if !group.is_empty() {
            r.tags.push(group);
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
        output.push(r);
    }
    Ok(output)
}

/// Generates a KDBX 4 protected by `password`.
pub fn export_kdbx(records: &[Record], password: &str) -> Result<Vec<u8>, IoError> {
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
        let mut root = db.root_mut();
        root.name = "neka".into();
        for r in records {
            let mut e = root.add_entry();
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
    let mut output = Vec::new();
    db.save(&mut output, key(password))
        .map_err(|_| IoError::KdbxWriteFailed)?;
    Ok(output)
}
