//! Armazenamento: leitura e escrita do arquivo `.neka` (a especificação do formato),
//! abertura por senha ou chave de recuperação, troca de senha e gravação
//! atômica.

mod arquivo;
mod atomico;
mod cofre;
mod erro;

pub use arquivo::{Arquivo, Slot, MAGIC, TAM_MAX_ARQUIVO, VERSAO_ATUAL};
pub use atomico::{backup, gravar_atomico, ler_arquivo, Passo};
pub use cofre::CofreAberto;
pub use erro::ErroArmazenamento;

#[doc(hidden)]
pub use atomico::gravar_atomico_com_falha;
