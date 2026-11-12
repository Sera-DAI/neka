//! Importação e exportação no nível da aplicação.

use super::{Cofre, ErroApi, NovaEntrada};
use crate::io::Registro;

/// Resultado de uma importação. Só contagens.
#[derive(Debug, Default, Clone, Copy, PartialEq, Eq)]
pub struct RelatorioImportacao {
    pub adicionadas: usize,
    /// Já existiam (mesmo título, usuário, endereço e senha).
    pub duplicadas: usize,
    /// Recusadas pela validação (campo grande demais, por exemplo).
    pub invalidas: usize,
}

fn mesma_conta(r: &Registro, e: &crate::model::Entrada) -> bool {
    r.title.trim().eq_ignore_ascii_case(e.title.trim())
        && r.username == e.username
        && r.url == e.url
        && r.password == e.password
}

impl Cofre {
    /// Adiciona os registros como entradas novas. Não grava: chame
    /// [`Cofre::salvar`] depois. Se nada for válido, o cofre não muda.
    pub fn importar(
        &mut self,
        registros: &[Registro],
        agora: i64,
    ) -> Result<RelatorioImportacao, ErroApi> {
        let mut rel = RelatorioImportacao::default();
        for r in registros {
            if self.entradas().iter().any(|e| mesma_conta(r, e)) {
                rel.duplicadas += 1;
                continue;
            }
            let nova = NovaEntrada {
                title: r.title.clone(),
                username: r.username.clone(),
                password: r.password.clone(),
                url: r.url.clone(),
                notes: r.notes.clone(),
                tags: r.tags.clone(),
                extra: r.extra.clone(),
            };
            match self.adicionar(nova, agora) {
                Ok(_) => rel.adicionadas += 1,
                Err(ErroApi::Modelo(_)) => rel.invalidas += 1,
                Err(e) => return Err(e),
            }
        }
        Ok(rel)
    }

    /// Todas as entradas como registros, para exportar.
    pub fn registros(&self) -> Vec<Registro> {
        self.entradas()
            .iter()
            .map(|e| Registro {
                title: e.title.clone(),
                username: e.username.clone(),
                password: e.password.clone(),
                url: e.url.clone(),
                notes: e.notes.clone(),
                tags: e.tags.clone(),
                extra: e.extra.clone(),
            })
            .collect()
    }
}
