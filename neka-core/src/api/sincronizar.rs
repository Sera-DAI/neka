//! Sincronização por pasta de nuvem (D3, D11, D12, D15).
//!
//! O cofre local é a fonte da verdade de cada dispositivo. A pasta (por
//! exemplo, dentro do Google Drive) só recebe cópias completas, gravadas de
//! forma atômica. A sincronização:
//!
//! 1. faz um backup datado do cofre local;
//! 2. lê todos os `*.neka` da pasta com o mesmo `vault_id` (inclusive as
//!    cópias de conflito que o serviço de nuvem cria);
//! 3. mescla cada um no cofre local (D11). Um arquivo de época de chave
//!    maior (senha trocada em outro dispositivo) passa a ser a base, e o
//!    cofre local adota a chave nova. Um arquivo de época menor é cópia
//!    velha: não é mesclado nem apagado (D15);
//! 4. remove lápides com mais de 180 dias (D12);
//! 5. grava o cofre local e publica a cópia na pasta;
//! 6. apaga as cópias de conflito já absorvidas.

use std::fs;
use std::path::{Path, PathBuf};

use super::{Cofre, ErroApi};
use crate::store::{gravar_atomico, ler_arquivo, Arquivo, CofreAberto, ErroArmazenamento};
use crate::sync::{mesclar, podar_lapides};

/// Quantos backups datados manter por cofre.
pub const BACKUPS_MANTIDOS: usize = 20;

/// O que a sincronização fez. Só caminhos e contagens, nada do conteúdo.
#[derive(Debug, Default, Clone, PartialEq, Eq)]
pub struct RelatorioSync {
    /// Arquivos da pasta mesclados no cofre local.
    pub mesclados: Vec<PathBuf>,
    /// Cópias de conflito apagadas depois de absorvidas.
    pub removidos: Vec<PathBuf>,
    /// Arquivos de época menor (senha antiga): deixados como estão.
    pub copias_velhas: Vec<PathBuf>,
    /// Arquivos de mesma época que a senha não abre (troca de senha
    /// concorrente em dois dispositivos): deixados como estão.
    pub nao_abertos: Vec<PathBuf>,
    /// Arquivos de outro cofre ou que não são cofres válidos: ignorados.
    pub ignorados: Vec<PathBuf>,
    /// Lápides com mais de 180 dias removidas.
    pub lapides_podadas: usize,
    /// O cofre local adotou a chave de um arquivo com senha mais nova.
    /// A senha mestra e a chave de recuperação passam a ser as dele.
    pub adotou_senha_nova: bool,
    /// Se o conteúdo local mudou com a mesclagem.
    pub alterou_local: bool,
    /// Backup feito antes de mexer em qualquer coisa.
    pub backup: PathBuf,
    /// Cópia publicada na pasta.
    pub publicado: PathBuf,
}

fn eh_cofre(p: &Path) -> bool {
    p.is_file()
        && p.extension()
            .and_then(|e| e.to_str())
            .is_some_and(|e| e.eq_ignore_ascii_case("neka"))
}

fn io(e: std::io::Error) -> ErroApi {
    ErroApi::Armazenamento(ErroArmazenamento::Io(e.kind()))
}

/// Backup datado em `<pasta do cofre>/backups/`, mantendo os
/// [`BACKUPS_MANTIDOS`] mais recentes.
fn backup_datado(caminho: &Path, agora: i64) -> Result<PathBuf, ErroApi> {
    let pasta = caminho.parent().unwrap_or(Path::new(".")).join("backups");
    fs::create_dir_all(&pasta).map_err(io)?;
    let base = caminho
        .file_stem()
        .and_then(|s| s.to_str())
        .unwrap_or("cofre")
        .to_owned();
    let destino = pasta.join(format!("{base}-{agora:015}.neka"));
    gravar_atomico(&destino, &ler_arquivo(caminho)?)?;

    let prefixo = format!("{base}-");
    let mut antigos: Vec<PathBuf> = fs::read_dir(&pasta)
        .map_err(io)?
        .filter_map(|e| e.ok().map(|e| e.path()))
        .filter(|p| {
            eh_cofre(p)
                && p.file_name()
                    .and_then(|n| n.to_str())
                    .is_some_and(|n| n.starts_with(&prefixo))
        })
        .collect();
    antigos.sort();
    let sobra = antigos.len().saturating_sub(BACKUPS_MANTIDOS);
    for p in &antigos[..sobra] {
        let _ = fs::remove_file(p);
    }
    Ok(destino)
}

impl Cofre {
    /// Sincroniza com a pasta `pasta`. `senha` é a senha mestra que abriu
    /// este cofre. Se outro dispositivo trocou a senha, a chamada falha com
    /// [`ErroApi::SenhaTrocadaEmOutroDispositivo`] sem alterar nada; a
    /// interface pede a senha nova e chama de novo com `senha_nova`.
    pub fn sincronizar(
        &mut self,
        pasta: &Path,
        senha: &[u8],
        senha_nova: Option<&[u8]>,
        agora: i64,
    ) -> Result<RelatorioSync, ErroApi> {
        let nome = self
            .caminho
            .file_name()
            .ok_or(ErroApi::PastaDeSyncInvalida)?
            .to_owned();
        let publicado = pasta.join(&nome);
        if !pasta.is_dir() {
            return Err(ErroApi::PastaDeSyncInvalida);
        }
        let mesma = |a: &Path, b: &Path| match (a.canonicalize(), b.canonicalize()) {
            (Ok(x), Ok(y)) => x == y,
            _ => false,
        };
        if mesma(pasta, self.caminho.parent().unwrap_or(Path::new("."))) {
            return Err(ErroApi::PastaDeSyncInvalida);
        }

        // Lê e classifica os arquivos da pasta antes de mudar qualquer coisa.
        let mut candidatos: Vec<(PathBuf, u32, Vec<u8>)> = Vec::new();
        let mut rel = RelatorioSync {
            publicado: publicado.clone(),
            ..RelatorioSync::default()
        };
        let mut lidos: Vec<PathBuf> = fs::read_dir(pasta)
            .map_err(io)?
            .filter_map(|e| e.ok().map(|e| e.path()))
            .filter(|p| eh_cofre(p))
            .collect();
        lidos.sort();
        for p in lidos {
            let Ok(bytes) = ler_arquivo(&p) else {
                rel.ignorados.push(p);
                continue;
            };
            match Arquivo::decodificar(&bytes) {
                Ok(a) if a.vault_id == self.vault_id() => {
                    if a.key_epoch < self.epoca_da_chave() {
                        rel.copias_velhas.push(p);
                    } else {
                        candidatos.push((p, a.key_epoch, bytes));
                    }
                }
                _ => rel.ignorados.push(p),
            }
        }
        // Época maior primeiro: se houver senha nova, ela vira a base.
        candidatos.sort_by(|a, b| b.1.cmp(&a.1).then_with(|| a.0.cmp(&b.0)));

        // Abre tudo antes de alterar o cofre: se faltar a senha nova, nada muda.
        let mut abertos: Vec<(PathBuf, CofreAberto)> = Vec::new();
        let epoca_local = self.epoca_da_chave();
        for (p, epoca, bytes) in candidatos {
            let tentativa = match senha_nova {
                Some(n) if epoca > epoca_local => CofreAberto::abrir_com_senha(&bytes, n),
                _ => CofreAberto::abrir_com_senha(&bytes, senha),
            };
            match tentativa {
                Ok(c) => abertos.push((p, c)),
                Err(_) if epoca > epoca_local => {
                    return Err(ErroApi::SenhaTrocadaEmOutroDispositivo)
                }
                Err(_) => rel.nao_abertos.push(p),
            }
        }

        rel.backup = backup_datado(&self.caminho, agora)?;

        for (p, remoto) in abertos {
            let mesclado = mesclar(self.aberto.corpo(), remoto.corpo());
            if remoto.epoca_da_chave() > self.epoca_da_chave() {
                let mut base = remoto;
                *base.corpo_mut() = mesclado;
                self.aberto = base;
                rel.adotou_senha_nova = true;
                rel.alterou_local = true;
            } else if &mesclado != self.aberto.corpo() {
                *self.aberto.corpo_mut() = mesclado;
                rel.alterou_local = true;
            }
            rel.mesclados.push(p);
        }

        let mut corpo = self.aberto.corpo().clone();
        rel.lapides_podadas = podar_lapides(&mut corpo, agora);
        if rel.lapides_podadas > 0 {
            *self.aberto.corpo_mut() = corpo;
            rel.alterou_local = true;
        }

        let bytes = self.aberto.serializar()?;
        gravar_atomico(&self.caminho, &bytes)?;
        if ler_arquivo(&publicado).ok().as_deref() != Some(bytes.as_slice()) {
            gravar_atomico(&publicado, &bytes)?;
        }

        for p in &rel.mesclados {
            if p.file_name() != Some(nome.as_os_str()) && fs::remove_file(p).is_ok() {
                rel.removidos.push(p.clone());
            }
        }
        Ok(rel)
    }
}

impl Cofre {
    /// Traz para este dispositivo um cofre que já existe (por exemplo, a
    /// cópia na pasta do Google Drive): confere que a credencial abre o
    /// arquivo e grava a cópia local em `destino`, que não pode existir.
    pub fn clonar(
        origem: &Path,
        destino: &Path,
        device_id: crate::model::Uuid,
        senha: &[u8],
    ) -> Result<Self, ErroApi> {
        let bytes = ler_arquivo(origem)?;
        CofreAberto::abrir_com_senha(&bytes, senha)?;
        if destino.exists() {
            return Err(ErroApi::ArquivoJaExiste);
        }
        if let Some(p) = destino.parent() {
            fs::create_dir_all(p).map_err(io)?;
        }
        gravar_atomico(destino, &bytes)?;
        Cofre::abrir(destino, device_id, &super::Credencial::Senha(senha))
    }
}
