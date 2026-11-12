use std::fs::{File, OpenOptions, TryLockError};
use std::path::{Path, PathBuf};

use super::{chave_de_recuperacao_de_texto, ErroApi};
use crate::crypto::{aleatorio, Chave, ParametrosKdf};
use crate::model::{Corpo, Entrada, Lapide, Uuid};
use crate::store::{gravar_atomico, ler_arquivo, CofreAberto};

/// Como desbloquear o cofre. Os segredos são emprestados: a interface é
/// quem zera os próprios buffers.
pub enum Credencial<'a> {
    Senha(&'a [u8]),
    /// Texto da chave de recuperação, como mostrado na criação.
    Recuperacao(&'a str),
}

/// Campos de uma entrada nova.
#[derive(Default)]
pub struct NovaEntrada {
    pub title: String,
    pub username: String,
    pub password: String,
    pub url: String,
    pub notes: String,
    pub tags: Vec<String>,
    pub extra: std::collections::BTreeMap<String, String>,
}

/// Cofre aberto, ligado ao arquivo local de onde veio. As alterações ficam
/// em memória até [`Cofre::salvar`].
pub struct Cofre {
    pub(super) aberto: CofreAberto,
    pub(super) caminho: PathBuf,
    device_id: Uuid,
    /// Trava exclusiva de `<cofre>.lock`, mantida enquanto o cofre estiver
    /// aberto: impede duas instâncias de salvarem ao mesmo tempo.
    _trava: File,
}

/// Pega a trava exclusiva do cofre, sem esperar.
fn travar(caminho: &Path) -> Result<File, ErroApi> {
    let mut nome = caminho.as_os_str().to_owned();
    nome.push(".lock");
    let f = OpenOptions::new()
        .read(true)
        .write(true)
        .create(true)
        .truncate(false)
        .open(PathBuf::from(nome))
        .map_err(|e| ErroApi::Armazenamento(e.into()))?;
    match f.try_lock() {
        Ok(()) => Ok(f),
        Err(TryLockError::WouldBlock) => Err(ErroApi::CofreEmUso),
        Err(TryLockError::Error(e)) => Err(ErroApi::Armazenamento(e.into())),
    }
}

impl std::fmt::Debug for Cofre {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Cofre")
            .field("caminho", &self.caminho)
            .finish_non_exhaustive()
    }
}

fn id_aleatorio() -> Result<Uuid, ErroApi> {
    let mut b = [0u8; 16];
    aleatorio(&mut b)?;
    Ok(uuid::Builder::from_random_bytes(b).into_uuid())
}

impl Cofre {
    /// Cria e grava um cofre novo em `caminho` (que não pode existir).
    /// Devolve o cofre e a chave de recuperação, a mostrar uma única vez
    /// (veja `chave_de_recuperacao_para_texto`).
    pub fn criar(
        caminho: &Path,
        device_id: Uuid,
        senha: &[u8],
        kdf: ParametrosKdf,
    ) -> Result<(Self, Chave), ErroApi> {
        if senha.is_empty() {
            return Err(ErroApi::SenhaVazia);
        }
        kdf.validar()?;
        let trava = travar(caminho)?;
        if caminho.exists() {
            return Err(ErroApi::ArquivoJaExiste);
        }
        let (aberto, recuperacao) = CofreAberto::criar(senha, kdf, Corpo::default())?;
        let mut c = Self {
            aberto,
            caminho: caminho.to_owned(),
            device_id,
            _trava: trava,
        };
        c.salvar()?;
        Ok((c, recuperacao))
    }

    pub fn abrir(
        caminho: &Path,
        device_id: Uuid,
        credencial: &Credencial<'_>,
    ) -> Result<Self, ErroApi> {
        let trava = travar(caminho)?;
        let bytes = ler_arquivo(caminho)?;
        let aberto = match credencial {
            Credencial::Senha(s) => CofreAberto::abrir_com_senha(&bytes, s)?,
            Credencial::Recuperacao(t) => {
                CofreAberto::abrir_com_recuperacao(&bytes, &chave_de_recuperacao_de_texto(t)?)?
            }
        };
        Ok(Self {
            aberto,
            caminho: caminho.to_owned(),
            device_id,
            _trava: trava,
        })
    }

    /// Grava no caminho do cofre, de forma atômica.
    pub fn salvar(&mut self) -> Result<(), ErroApi> {
        let bytes = self.aberto.serializar()?;
        gravar_atomico(&self.caminho, &bytes)?;
        Ok(())
    }

    pub fn vault_id(&self) -> Uuid {
        self.aberto.vault_id()
    }

    pub fn entradas(&self) -> &[Entrada] {
        &self.aberto.corpo().entries
    }

    pub fn entrada(&self, id: Uuid) -> Result<&Entrada, ErroApi> {
        self.entradas()
            .iter()
            .find(|e| e.id == id)
            .ok_or(ErroApi::EntradaNaoEncontrada)
    }

    fn proximo_clock(&mut self) -> u64 {
        let corpo = self.aberto.corpo_mut();
        corpo.device_clock = corpo.device_clock.saturating_add(1);
        corpo.device_clock
    }

    pub fn adicionar(&mut self, nova: NovaEntrada, agora: i64) -> Result<Uuid, ErroApi> {
        let id = id_aleatorio()?;
        let mut e = Entrada::nova(id, self.device_id, nova.title, agora, 0);
        e.username = nova.username;
        e.password = nova.password;
        e.url = nova.url;
        e.notes = nova.notes;
        e.tags = nova.tags;
        e.extra = nova.extra;
        e.validar()?;
        e.clock = self.proximo_clock();
        self.aberto.corpo_mut().entries.push(e);
        Ok(id)
    }

    /// Edita uma entrada. A mudança de senha vai para o histórico. O `id` não
    /// pode mudar. Nada é alterado se a validação falhar.
    pub fn editar(
        &mut self,
        id: Uuid,
        alterar: impl FnOnce(&mut Entrada),
        agora: i64,
    ) -> Result<(), ErroApi> {
        let atual = self.entrada(id)?;
        let mut copia = atual.clone();
        alterar(&mut copia);
        copia.id = id;
        if copia.password != atual.password {
            let nova = std::mem::replace(&mut copia.password, atual.password.clone());
            copia.history = atual.history.clone();
            copia.trocar_senha(nova, agora);
        }
        copia.updated_at = agora;
        copia.created_at = atual.created_at;
        copia.device_id = self.device_id;
        copia.validar()?;
        copia.clock = self.proximo_clock();
        let corpo = self.aberto.corpo_mut();
        let pos = corpo
            .entries
            .iter()
            .position(|e| e.id == id)
            .ok_or(ErroApi::EntradaNaoEncontrada)?;
        corpo.entries[pos] = copia;
        Ok(())
    }

    /// Remove a entrada e deixa uma lápide (para a exclusão se propagar).
    pub fn remover(&mut self, id: Uuid, agora: i64) -> Result<(), ErroApi> {
        self.entrada(id)?;
        let clock = self.proximo_clock();
        let device_id = self.device_id;
        let corpo = self.aberto.corpo_mut();
        corpo.entries.retain(|e| e.id != id);
        corpo.tombstones.push(Lapide {
            id,
            deleted_at: agora,
            clock,
            device_id,
        });
        Ok(())
    }

    /// Troca a senha mestra **e gira a chave de dados** (D15): a senha antiga
    /// e a chave de recuperação antiga deixam de abrir o arquivo novo, que é
    /// recifrado inteiro. Devolve a **nova chave de recuperação**, a mostrar
    /// uma única vez (a antiga deixa de valer). Não grava: chame
    /// [`Cofre::salvar`] logo em seguida.
    ///
    /// Não alcança cópias antigas do arquivo: quem tem a senha antiga e uma
    /// cópia antiga ainda lê o que havia nela.
    pub fn trocar_senha_mestra(
        &mut self,
        nova: &[u8],
        kdf: ParametrosKdf,
    ) -> Result<Chave, ErroApi> {
        if nova.is_empty() {
            return Err(ErroApi::SenhaVazia);
        }
        kdf.validar()?;
        Ok(self.aberto.trocar_senha(nova, kdf)?)
    }

    /// Geração da chave de dados: sobe a cada troca de senha mestra.
    pub fn epoca_da_chave(&self) -> u32 {
        self.aberto.epoca_da_chave()
    }
}
