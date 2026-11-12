//! CSV. A exportação usa as colunas do próprio neka. A importação reconhece
//! o cabeçalho do neka, do Bitwarden, do Chrome/Edge, do Firefox, do
//! KeePass/KeePassXC e do 1Password, pelo nome das colunas.

use std::collections::BTreeMap;

use super::{ErroIo, Registro};

const COLUNAS: [&str; 6] = ["title", "username", "password", "url", "notes", "tags"];

#[derive(Clone, Copy, PartialEq, Eq)]
enum Campo {
    Titulo,
    Usuario,
    Senha,
    Url,
    Notas,
    Etiquetas,
    Grupo,
    Totp,
}

/// Nome de coluna normalizado → campo.
fn campo(nome: &str) -> Option<Campo> {
    let n: String = nome
        .trim()
        .trim_start_matches('\u{feff}')
        .to_lowercase()
        .chars()
        .filter(|c| c.is_alphanumeric())
        .collect();
    Some(match n.as_str() {
        "title" | "name" | "titulo" | "nome" | "account" => Campo::Titulo,
        "username" | "loginusername" | "user" | "login" | "usuario" | "email" => Campo::Usuario,
        "password" | "loginpassword" | "senha" => Campo::Senha,
        "url" | "loginuri" | "website" | "uri" | "endereco" | "site" => Campo::Url,
        "notes" | "note" | "notas" | "comments" => Campo::Notas,
        "tags" | "etiquetas" => Campo::Etiquetas,
        "folder" | "group" | "grupo" | "pasta" => Campo::Grupo,
        "totp" | "logintotp" | "otpauth" | "otp" => Campo::Totp,
        _ => return None,
    })
}

/// Lê um CSV com cabeçalho. Linhas sem título ganham o endereço ou o
/// usuário como título; linhas totalmente vazias são puladas.
pub fn importar_csv(bytes: &[u8]) -> Result<Vec<Registro>, ErroIo> {
    let mut leitor = ::csv::ReaderBuilder::new()
        .flexible(true)
        .from_reader(bytes);
    let cab = leitor.headers().map_err(|_| ErroIo::CsvInvalido)?.clone();
    let mapa: Vec<Option<Campo>> = cab.iter().map(campo).collect();
    let extras: Vec<Option<String>> = cab
        .iter()
        .map(|n| {
            n.strip_prefix("extra:")
                .filter(|e| !e.is_empty())
                .map(str::to_owned)
        })
        .collect();
    let tem = |c| mapa.contains(&Some(c));
    if !(tem(Campo::Titulo) || tem(Campo::Url)) || !(tem(Campo::Senha) || tem(Campo::Usuario)) {
        return Err(ErroIo::CsvSemColunas);
    }
    let mut saida = Vec::new();
    for linha in leitor.records() {
        let linha = linha.map_err(|_| ErroIo::CsvInvalido)?;
        let mut r = Registro::default();
        for (i, valor) in linha.iter().enumerate() {
            if let (Some(Some(nome)), false) = (extras.get(i), valor.is_empty()) {
                r.extra.insert(nome.clone(), valor.to_owned());
                continue;
            }
            let Some(Some(c)) = mapa.get(i) else { continue };
            if valor.is_empty() {
                continue;
            }
            match c {
                Campo::Titulo if r.title.is_empty() => r.title = valor.to_owned(),
                Campo::Usuario if r.username.is_empty() => r.username = valor.to_owned(),
                Campo::Senha if r.password.is_empty() => r.password = valor.to_owned(),
                Campo::Url if r.url.is_empty() => r.url = valor.to_owned(),
                Campo::Notas if r.notes.is_empty() => r.notes = valor.to_owned(),
                Campo::Etiquetas => r.tags.extend(
                    valor
                        .split([';', ','])
                        .map(str::trim)
                        .filter(|t| !t.is_empty())
                        .map(str::to_owned),
                ),
                Campo::Grupo => {
                    r.tags.push(valor.trim().to_owned());
                }
                Campo::Totp => {
                    r.extra.insert("totp".into(), valor.to_owned());
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
        saida.push(r);
    }
    Ok(saida)
}

/// Gera CSV **em claro** com as colunas do neka (etiquetas separadas por
/// `;`). Campos extras viram colunas `extra:<nome>`.
pub fn exportar_csv(registros: &[Registro]) -> Result<Vec<u8>, ErroIo> {
    let extras: Vec<String> = {
        let mut v: Vec<String> = registros
            .iter()
            .flat_map(|r| r.extra.keys().cloned())
            .collect();
        v.sort();
        v.dedup();
        v
    };
    let mut w = ::csv::Writer::from_writer(Vec::new());
    let mut cab: Vec<String> = COLUNAS.iter().map(|s| (*s).to_owned()).collect();
    cab.extend(extras.iter().map(|e| format!("extra:{e}")));
    w.write_record(&cab).map_err(|_| ErroIo::CsvInvalido)?;
    let vazio = BTreeMap::new();
    for r in registros {
        let tags = r.tags.join(";");
        let mut linha: Vec<&str> =
            vec![&r.title, &r.username, &r.password, &r.url, &r.notes, &tags];
        let ex = if r.extra.is_empty() { &vazio } else { &r.extra };
        for e in &extras {
            linha.push(ex.get(e).map(String::as_str).unwrap_or(""));
        }
        w.write_record(&linha).map_err(|_| ErroIo::CsvInvalido)?;
    }
    w.into_inner().map_err(|_| ErroIo::CsvInvalido)
}
