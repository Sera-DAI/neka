//! Gravação atômica: temporário na mesma pasta, fsync, renomeação.

use std::fs::{self, File, OpenOptions};
use std::io::{self, Read, Write};
use std::path::{Path, PathBuf};

use super::{ErroArmazenamento, TAM_MAX_ARQUIVO};

/// Passos da gravação, para injetar falhas nos testes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Passo {
    CriarTemporario,
    Escrever,
    Sincronizar,
    Renomear,
    SincronizarPasta,
}

/// Temporário com nome imprevisível, na mesma pasta do destino.
fn temporario(destino: &Path) -> Result<PathBuf, ErroArmazenamento> {
    let mut r = [0u8; 8];
    crate::crypto::aleatorio(&mut r)?;
    let hex: String = r.iter().map(|b| format!("{b:02x}")).collect();
    Ok(com_sufixo(destino, &format!(".{hex}.tmp")))
}

fn com_sufixo(destino: &Path, sufixo: &str) -> PathBuf {
    let mut s = destino.as_os_str().to_owned();
    s.push(sufixo);
    PathBuf::from(s)
}

/// Grava `bytes` em `destino` sem nunca deixar um arquivo pela metade.
pub fn gravar_atomico(destino: &Path, bytes: &[u8]) -> Result<(), ErroArmazenamento> {
    gravar_atomico_com_falha(destino, bytes, |_| Ok(()))
}

/// Igual a [`gravar_atomico`], chamando `antes` antes de cada passo. Um erro
/// devolvido por `antes` simula a falha daquele passo. Só para testes.
pub fn gravar_atomico_com_falha(
    destino: &Path,
    bytes: &[u8],
    mut antes: impl FnMut(Passo) -> io::Result<()>,
) -> Result<(), ErroArmazenamento> {
    let tmp = temporario(destino)?;
    let mut criado = false;
    let r = (|| -> io::Result<()> {
        antes(Passo::CriarTemporario)?;
        let mut f = abrir_novo(&tmp)?;
        criado = true;
        antes(Passo::Escrever)?;
        f.write_all(bytes)?;
        antes(Passo::Sincronizar)?;
        f.sync_all()?;
        drop(f);
        antes(Passo::Renomear)?;
        fs::rename(&tmp, destino)?;
        Ok(())
    })();
    if let Err(e) = r {
        // Só remove o que este chamador criou (nunca um arquivo alheio).
        if criado {
            let _ = fs::remove_file(&tmp);
        }
        return Err(e.into());
    }
    // Depois da renomeação o arquivo novo já está no lugar; sincronizar a
    // pasta só torna a renomeação durável (no Windows não se aplica). Alguns
    // sistemas de arquivos recusam o fsync de pasta: a falha aqui não desfaz
    // a gravação, então é ignorada (limitação em a documentação de segurança).
    let _ = antes(Passo::SincronizarPasta);
    let _ = sincronizar_pasta(destino);
    Ok(())
}

/// Cria o temporário só se não existir (`create_new`: não segue link simbólico
/// plantado) e, no Unix, com permissão 0600.
fn abrir_novo(caminho: &Path) -> io::Result<File> {
    let mut o = OpenOptions::new();
    o.write(true).create_new(true);
    #[cfg(unix)]
    {
        use std::os::unix::fs::OpenOptionsExt;
        o.mode(0o600);
    }
    o.open(caminho)
}

#[cfg(unix)]
fn sincronizar_pasta(destino: &Path) -> io::Result<()> {
    match destino.parent() {
        Some(p) if !p.as_os_str().is_empty() => File::open(p)?.sync_all(),
        _ => File::open(".")?.sync_all(),
    }
}

#[cfg(not(unix))]
fn sincronizar_pasta(_destino: &Path) -> io::Result<()> {
    Ok(())
}

/// Lê um arquivo de cofre, recusando o que passar do limite de tamanho.
pub fn ler_arquivo(caminho: &Path) -> Result<Vec<u8>, ErroArmazenamento> {
    let f = File::open(caminho)?;
    let mut v = Vec::new();
    f.take(TAM_MAX_ARQUIVO as u64 + 1).read_to_end(&mut v)?;
    if v.len() > TAM_MAX_ARQUIVO {
        return Err(ErroArmazenamento::ArquivoGrandeDemais);
    }
    Ok(v)
}

/// Copia o cofre para `<destino>.bak` (antes de migrações), também de forma
/// atômica. Devolve o caminho da cópia.
pub fn backup(caminho: &Path) -> Result<PathBuf, ErroArmazenamento> {
    let bytes = ler_arquivo(caminho)?;
    let bak = com_sufixo(caminho, ".bak");
    gravar_atomico(&bak, &bytes)?;
    Ok(bak)
}
