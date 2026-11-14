//! CSV. Export uses neka's own columns. Import recognizes
//! the header of neka, Bitwarden, Chrome/Edge, Firefox,
//! KeePass/KeePassXC and 1Password, by column name (Portuguese column names are accepted too).

use std::collections::BTreeMap;

use super::{IoError, Record};

const COLUMNS: [&str; 6] = ["title", "username", "password", "url", "notes", "tags"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Title,
    Username,
    Password,
    Url,
    Notes,
    Tags,
    Group,
    Totp,
}

/// Normalized column name → field.
fn field(name: &str) -> Option<Field> {
    let n: String = name
        .trim()
        .trim_start_matches('\u{feff}')
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    Some(match n.as_str() {
        "title" | "name" | "titulo" | "nome" | "account" => Field::Title,
        "username" | "loginusername" | "user" | "login" | "usuario" | "email" => Field::Username,
        "password" | "loginpassword" | "senha" => Field::Password,
        "url" | "loginuri" | "website" | "uri" | "endereco" | "site" => Field::Url,
        "notes" | "note" | "notas" | "comments" => Field::Notes,
        "tags" | "etiquetas" => Field::Tags,
        "folder" | "group" | "grupo" | "pasta" => Field::Group,
        "totp" | "logintotp" | "otpauth" | "otp" => Field::Totp,
        _ => return None,
    })
}

/// Reads a CSV with a header. Rows without a title get the address or the
/// username as the title; fully empty rows are skipped.
pub fn import_csv(bytes: &[u8]) -> Result<Vec<Record>, IoError> {
    let mut reader = ::csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(bytes);
    let header = reader.headers().map_err(|_| IoError::InvalidCsv)?.clone();
    let mapping: Vec<Option<Field>> = header.iter().map(field).collect();
    let extras: Vec<Option<String>> = header
        .iter()
        .map(|n| {
            n.strip_prefix("extra:")
                .filter(|e| !e.is_empty())
                .map(str::to_owned)
        })
        .collect();
    let has = |c| mapping.contains(&Some(c));
    if !(has(Field::Title) || has(Field::Url)) || !(has(Field::Password) || has(Field::Username)) {
        return Err(IoError::CsvNoColumns);
    }
    let mut output = Vec::new();
    for line in reader.records() {
        let line = line.map_err(|_| IoError::InvalidCsv)?;
        let mut r = Record::default();
        for (i, value) in line.iter().enumerate() {
            if let (Some(Some(name)), false) = (extras.get(i), value.is_empty()) {
                r.extra.insert(name.clone(), value.to_owned());
                continue;
            }
            let Some(Some(c)) = mapping.get(i) else {
                continue;
            };
            if value.is_empty() {
                continue;
            }
            match c {
                Field::Title if r.title.is_empty() => r.title = value.to_owned(),
                Field::Username if r.username.is_empty() => r.username = value.to_owned(),
                Field::Password if r.password.is_empty() => r.password = value.to_owned(),
                Field::Url if r.url.is_empty() => r.url = value.to_owned(),
                Field::Notes if r.notes.is_empty() => r.notes = value.to_owned(),
                Field::Tags => r.tags.extend(
                    value
                        .split([';', ','])
                        .map(str::trim)
                        .filter(|t| !t.is_empty())
                        .map(str::to_owned),
                ),
                Field::Group => {
                    r.tags.push(value.trim().to_owned());
                }
                Field::Totp => {
                    r.extra.insert("totp".into(), value.to_owned());
                }
                _ => {}
            }
        }
        if r.title.trim().is_empty() {
            r.title = if !r.url.is_empty() {
                r.url.clone()
            } else {
                r.username.clone()
            };
        }
        if r.title.trim().is_empty() && r.password.is_empty() {
            continue;
        }
        r.tags.sort();
        r.tags.dedup();
        output.push(r);
    }
    Ok(output)
}

/// Generates **plaintext** CSV with neka's columns (tags separated by
/// `;`). Extra fields become `extra:<name>` columns.
pub fn export_csv(records: &[Record]) -> Result<Vec<u8>, IoError> {
    let extras: Vec<String> = {
        let mut v: Vec<String> = records
            .iter()
            .flat_map(|r| r.extra.keys().cloned())
            .collect();
        v.sort();
        v.dedup();
        v
    };
    let mut w = ::csv::Writer::from_writer(Vec::new());
    let mut header: Vec<String> = COLUMNS.iter().map(|s| (*s).to_owned()).collect();
    header.extend(extras.iter().map(|e| format!("extra:{e}")));
    w.write_record(&header).map_err(|_| IoError::InvalidCsv)?;
    let empty = BTreeMap::new();
    for r in records {
        let tags = r.tags.join(";");
        let mut line: Vec<&str> = vec![&r.title, &r.username, &r.password, &r.url, &r.notes, &tags];
        let ex = if r.extra.is_empty() { &empty } else { &r.extra };
        for e in &extras {
            line.push(ex.get(e).map(String::as_str).unwrap_or(""));
        }
        w.write_record(&line).map_err(|_| IoError::InvalidCsv)?;
    }
    w.into_inner().map_err(|_| IoError::InvalidCsv)
}
