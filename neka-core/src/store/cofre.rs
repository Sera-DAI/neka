use uuid::Uuid;
use zeroize::Zeroize;

use super::arquivo::{Arquivo, Slot};
use super::ErroArmazenamento;
use crate::crypto::{
    abrir, aleatorio, chave_da_recuperacao, chave_da_senha, desembrulhar, embrulhar, selar, Chave,
    Nonce, ParametrosKdf, Sal, TipoSlot,
};
use crate::model::Corpo;

/// Cofre decifrado em memória.
///
/// Guarda o corpo cifrado da última leitura ou gravação: se o corpo não foi
/// alterado ([`CofreAberto::corpo_mut`] não foi chamado), a próxima gravação
/// reaproveita os mesmos bytes. Assim trocar a senha não muda o corpo.
pub struct CofreAberto {
    arquivo: Arquivo,
    chave_dados: Chave,
    corpo: Corpo,
    alterado: bool,
}

impl std::fmt::Debug for CofreAberto {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("CofreAberto")
            .field("vault_id", &self.arquivo.vault_id)
            .field("segredos", &"<ocultos>")
            .finish()
    }
}

fn uuid_aleatorio() -> Result<Uuid, ErroArmazenamento> {
    let mut b = [0u8; 16];
    aleatorio(&mut b)?;
    Ok(uuid::Builder::from_random_bytes(b).into_uuid())
}

fn selar_corpo(
    chave: &Chave,
    vault_id: &Uuid,
    epoca: u32,
    corpo: &Corpo,
) -> Result<(Nonce, Vec<u8>), ErroArmazenamento> {
    let mut claro = corpo.para_cbor()?;
    let nonce = Nonce::aleatorio()?;
    let len = (claro.len() + crate::crypto::TAM_ETIQUETA) as u64;
    let r = selar(
        chave,
        &nonce,
        &Arquivo::aad_corpo(vault_id, epoca, &nonce, len),
        &claro,
    );
    claro.zeroize();
    Ok((nonce, r?))
}

impl CofreAberto {
    /// Cria um cofre novo. Devolve o cofre e a chave de recuperação, que deve
    /// ser mostrada uma única vez e nunca gravada em claro.
    pub fn criar(
        senha: &[u8],
        kdf: ParametrosKdf,
        corpo: Corpo,
    ) -> Result<(Self, Chave), ErroArmazenamento> {
        corpo.validar()?;
        let vault_id = uuid_aleatorio()?;
        let sal = Sal::aleatorio()?;
        let chave_dados = Chave::aleatoria()?;
        let recuperacao = Chave::aleatoria()?;
        let k_senha = chave_da_senha(senha, &sal, &kdf)?;
        let slots = vec![
            Slot {
                tipo: TipoSlot::Senha,
                chave: embrulhar(&k_senha, &chave_dados, &vault_id, 0, TipoSlot::Senha)?,
            },
            Slot {
                tipo: TipoSlot::Recuperacao,
                chave: embrulhar(
                    &chave_da_recuperacao(&recuperacao),
                    &chave_dados,
                    &vault_id,
                    0,
                    TipoSlot::Recuperacao,
                )?,
            },
        ];
        let (body_nonce, body) = selar_corpo(&chave_dados, &vault_id, 0, &corpo)?;
        let arquivo = Arquivo {
            vault_id,
            key_epoch: 0,
            kdf,
            sal,
            slots,
            body_nonce,
            body,
        };
        Ok((
            Self {
                arquivo,
                chave_dados,
                corpo,
                alterado: false,
            },
            recuperacao,
        ))
    }

    pub fn abrir_com_senha(bytes: &[u8], senha: &[u8]) -> Result<Self, ErroArmazenamento> {
        let arquivo = Arquivo::decodificar(bytes)?;
        let k = chave_da_senha(senha, &arquivo.sal, &arquivo.kdf)?;
        Self::abrir_com(arquivo, &k, TipoSlot::Senha)
    }

    pub fn abrir_com_recuperacao(
        bytes: &[u8],
        recuperacao: &Chave,
    ) -> Result<Self, ErroArmazenamento> {
        let arquivo = Arquivo::decodificar(bytes)?;
        Self::abrir_com(
            arquivo,
            &chave_da_recuperacao(recuperacao),
            TipoSlot::Recuperacao,
        )
    }

    fn abrir_com(
        arquivo: Arquivo,
        embrulho: &Chave,
        tipo: TipoSlot,
    ) -> Result<Self, ErroArmazenamento> {
        let slot = arquivo
            .slots
            .iter()
            .find(|s| s.tipo == tipo)
            .ok_or(ErroArmazenamento::SemCompartimento)?;
        let chave_dados = desembrulhar(
            embrulho,
            &slot.chave,
            &arquivo.vault_id,
            arquivo.key_epoch,
            tipo,
        )?;
        let aad = Arquivo::aad_corpo(
            &arquivo.vault_id,
            arquivo.key_epoch,
            &arquivo.body_nonce,
            arquivo.body.len() as u64,
        );
        let mut claro = abrir(&chave_dados, &arquivo.body_nonce, &aad, &arquivo.body)?;
        let corpo = Corpo::de_cbor(&claro);
        claro.zeroize();
        Ok(Self {
            arquivo,
            chave_dados,
            corpo: corpo?,
            alterado: false,
        })
    }

    pub fn vault_id(&self) -> Uuid {
        self.arquivo.vault_id
    }

    /// Geração da chave de dados (sobe a cada troca de senha mestra).
    pub fn epoca_da_chave(&self) -> u32 {
        self.arquivo.key_epoch
    }

    pub fn parametros_kdf(&self) -> ParametrosKdf {
        self.arquivo.kdf
    }

    pub fn corpo(&self) -> &Corpo {
        &self.corpo
    }

    /// Acesso para alterar o corpo; marca o corpo para ser recifrado.
    pub fn corpo_mut(&mut self) -> &mut Corpo {
        self.alterado = true;
        &mut self.corpo
    }

    /// Troca a senha mestra **girando a chave de dados** (D15): chave de dados
    /// nova, época +1, sal novo, chave de recuperação nova e corpo recifrado na
    /// próxima gravação. Quem tem só a senha antiga (ou a recuperação antiga)
    /// não abre mais o arquivo novo. Devolve a nova chave de recuperação, a
    /// mostrar uma única vez. Nada muda se algum passo falhar.
    pub fn trocar_senha(
        &mut self,
        nova: &[u8],
        kdf: ParametrosKdf,
    ) -> Result<Chave, ErroArmazenamento> {
        let epoca = self
            .arquivo
            .key_epoch
            .checked_add(1)
            .ok_or(ErroArmazenamento::FormatoInvalido)?;
        let vault_id = self.arquivo.vault_id;
        let sal = Sal::aleatorio()?;
        let k_senha = chave_da_senha(nova, &sal, &kdf)?;
        let chave_dados = Chave::aleatoria()?;
        let recuperacao = Chave::aleatoria()?;
        let slots = vec![
            Slot {
                tipo: TipoSlot::Senha,
                chave: embrulhar(&k_senha, &chave_dados, &vault_id, epoca, TipoSlot::Senha)?,
            },
            Slot {
                tipo: TipoSlot::Recuperacao,
                chave: embrulhar(
                    &chave_da_recuperacao(&recuperacao),
                    &chave_dados,
                    &vault_id,
                    epoca,
                    TipoSlot::Recuperacao,
                )?,
            },
        ];
        self.arquivo.key_epoch = epoca;
        self.arquivo.sal = sal;
        self.arquivo.kdf = kdf;
        self.arquivo.slots = slots;
        self.chave_dados = chave_dados; // a antiga é zerada ao ser descartada
        self.alterado = true; // o corpo é recifrado com a chave nova
        Ok(recuperacao)
    }

    /// Bytes do arquivo para gravar. Se o corpo mudou, recifra com nonce novo.
    pub fn serializar(&mut self) -> Result<Vec<u8>, ErroArmazenamento> {
        if self.alterado {
            let (nonce, body) = selar_corpo(
                &self.chave_dados,
                &self.arquivo.vault_id,
                self.arquivo.key_epoch,
                &self.corpo,
            )?;
            self.arquivo.body_nonce = nonce;
            self.arquivo.body = body;
            self.alterado = false;
        }
        let bytes = self.arquivo.codificar();
        // A leitura recusa arquivos acima do limite; gravar um cofre que não
        // abre mais seria perda de dados.
        if bytes.len() > super::TAM_MAX_ARQUIVO {
            return Err(ErroArmazenamento::ArquivoGrandeDemais);
        }
        Ok(bytes)
    }
}

#[cfg(test)]
mod testes {
    use super::*;

    const RAPIDO: ParametrosKdf = ParametrosKdf {
        mem_kib: ParametrosKdf::MEM_MIN_KIB,
        iters: 1,
        lanes: 1,
    };

    /// Prova a revogação (D15) no nível da chave: depois da troca, a chave de
    /// dados antiga não decifra o corpo novo nem desembrulha os slots novos.
    #[test]
    fn chave_de_dados_antiga_nao_decifra_o_corpo_novo() {
        let (mut c, _) = CofreAberto::criar(b"senha ficticia", RAPIDO, Corpo::default()).unwrap();
        let antiga = Chave::de_bytes(*c.chave_dados.como_bytes());
        c.trocar_senha(b"outra senha ficticia", RAPIDO).unwrap();
        c.serializar().unwrap();
        assert_ne!(antiga.como_bytes(), c.chave_dados.como_bytes());
        let a = &c.arquivo;
        let aad = Arquivo::aad_corpo(&a.vault_id, a.key_epoch, &a.body_nonce, a.body.len() as u64);
        assert!(abrir(&antiga, &a.body_nonce, &aad, &a.body).is_err());
        assert!(abrir(&c.chave_dados, &a.body_nonce, &aad, &a.body).is_ok());
    }

    /// Se a troca falhar (parâmetros inválidos), nada muda.
    #[test]
    fn troca_que_falha_nao_altera_o_cofre() {
        let (mut c, _) = CofreAberto::criar(b"senha ficticia", RAPIDO, Corpo::default()).unwrap();
        let antes = c.serializar().unwrap();
        let ruim = ParametrosKdf {
            mem_kib: 1,
            ..RAPIDO
        };
        assert!(c.trocar_senha(b"x", ruim).is_err());
        assert_eq!(c.epoca_da_chave(), 0);
        assert_eq!(c.serializar().unwrap(), antes);
    }
}
