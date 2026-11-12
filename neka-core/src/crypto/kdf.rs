use argon2::{Algorithm, Argon2, Params, Version};
use hkdf::Hkdf;
use sha2::Sha256;

use super::{aleatorio, Chave, ErroCripto, TAM_CHAVE};

pub const TAM_SAL: usize = 16;

/// Contexto HKDF da chave de recuperação (a especificação do formato).
const CONTEXTO_RECUPERACAO: &[u8] = b"neka-v1-recovery";

/// Sal aleatório da senha mestra.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Sal(pub [u8; TAM_SAL]);

impl Sal {
    pub fn aleatorio() -> Result<Self, ErroCripto> {
        let mut s = [0; TAM_SAL];
        aleatorio(&mut s)?;
        Ok(Self(s))
    }
}

/// Parâmetros do Argon2id gravados no cabeçalho (D8).
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ParametrosKdf {
    pub mem_kib: u32,
    pub iters: u32,
    pub lanes: u32,
}

impl ParametrosKdf {
    /// Memória mínima aceita: 19 MiB (piso da OWASP para Argon2id).
    pub const MEM_MIN_KIB: u32 = 19 * 1024;
    /// Memória máxima aceita: 1 GiB, para um arquivo malicioso não esgotar a máquina.
    pub const MEM_MAX_KIB: u32 = 1024 * 1024;
    pub const ITERS_MIN: u32 = 1;
    pub const ITERS_MAX: u32 = 16;
    pub const LANES_MIN: u32 = 1;
    pub const LANES_MAX: u32 = 16;

    /// Proposta inicial da D8: 256 MiB, 3 iterações, 4 threads.
    pub const PADRAO: Self = Self {
        mem_kib: 256 * 1024,
        iters: 3,
        lanes: 4,
    };

    pub fn validar(&self) -> Result<(), ErroCripto> {
        let ok = (Self::MEM_MIN_KIB..=Self::MEM_MAX_KIB).contains(&self.mem_kib)
            && (Self::ITERS_MIN..=Self::ITERS_MAX).contains(&self.iters)
            && (Self::LANES_MIN..=Self::LANES_MAX).contains(&self.lanes);
        if ok {
            Ok(())
        } else {
            Err(ErroCripto::ParametrosForaDosLimites)
        }
    }
}

/// Argon2id v0x13 sem validação de limites; uso interno e vetores de teste.
fn argon2id_bruto(
    senha: &[u8],
    sal: &[u8],
    segredo: Option<&[u8]>,
    p: &ParametrosKdf,
    saida: &mut [u8],
) -> Result<(), ErroCripto> {
    let params = Params::new(p.mem_kib, p.iters, p.lanes, Some(saida.len()))
        .map_err(|_| ErroCripto::ParametrosForaDosLimites)?;
    let a = match segredo {
        Some(s) => Argon2::new_with_secret(s, Algorithm::Argon2id, Version::V0x13, params)
            .map_err(|_| ErroCripto::ParametrosForaDosLimites)?,
        None => Argon2::new(Algorithm::Argon2id, Version::V0x13, params),
    };
    a.hash_password_into(senha, sal, saida)
        .map_err(|_| ErroCripto::FalhaKdf)
}

/// Chave de embrulho do compartimento da senha mestra. Recusa parâmetros
/// fora dos limites antes de alocar memória.
pub fn chave_da_senha(senha: &[u8], sal: &Sal, p: &ParametrosKdf) -> Result<Chave, ErroCripto> {
    p.validar()?;
    let mut saida = [0u8; TAM_CHAVE];
    let r = argon2id_bruto(senha, &sal.0, None, p, &mut saida);
    let chave = Chave::de_bytes(saida);
    zeroize::Zeroize::zeroize(&mut saida);
    r.map(|_| chave)
}

/// Chave de embrulho do compartimento de recuperação:
/// HKDF-SHA256 sem sal, com `info = "neka-v1-recovery"`.
pub fn chave_da_recuperacao(recuperacao: &Chave) -> Chave {
    let mut saida = [0u8; TAM_CHAVE];
    Hkdf::<Sha256>::new(None, recuperacao.como_bytes())
        .expand(CONTEXTO_RECUPERACAO, &mut saida)
        .expect("32 bytes cabem no HKDF-SHA256");
    let chave = Chave::de_bytes(saida);
    zeroize::Zeroize::zeroize(&mut saida);
    chave
}

#[cfg(test)]
mod testes {
    use super::*;

    fn hex(s: &str) -> Vec<u8> {
        (0..s.len())
            .step_by(2)
            .map(|i| u8::from_str_radix(&s[i..i + 2], 16).unwrap())
            .collect()
    }

    /// RFC 9106, seção 5.3 (Argon2id). Usa segredo e dado associado, que o
    /// neka não usa; por isso o teste monta o contexto direto.
    #[test]
    fn vetor_conhecido_argon2id_rfc9106() {
        let params = Params::new(32, 3, 4, Some(32)).unwrap();
        let mut pb = argon2::ParamsBuilder::new();
        pb.m_cost(params.m_cost())
            .t_cost(3)
            .p_cost(4)
            .output_len(32)
            .data(argon2::AssociatedData::new(&[4u8; 12]).unwrap());
        let params = pb.build().unwrap();
        let a = Argon2::new_with_secret(&[3u8; 8], Algorithm::Argon2id, Version::V0x13, params)
            .unwrap();
        let mut saida = [0u8; 32];
        a.hash_password_into(&[1u8; 32], &[2u8; 16], &mut saida)
            .unwrap();
        assert_eq!(
            saida.to_vec(),
            hex("0d640df58d78766c08c037a34a8b53c9d01ef0452d75b65eb52520e96b01e659")
        );
    }

    /// RFC 5869, caso de teste 1 (HKDF-SHA256).
    #[test]
    fn vetor_conhecido_hkdf_rfc5869() {
        let mut okm = [0u8; 42];
        Hkdf::<Sha256>::new(Some(&hex("000102030405060708090a0b0c")), &[0x0b; 22])
            .expand(&hex("f0f1f2f3f4f5f6f7f8f9"), &mut okm)
            .unwrap();
        assert_eq!(
            okm.to_vec(),
            hex("3cb25f25faacd57a90434f64d0362f2a2d2d0a90cf1a5a4c5db02d56ecc4c5bf34007208d5b887185865")
        );
    }

    #[test]
    fn argon2id_bruto_e_deterministico() {
        let p = ParametrosKdf {
            mem_kib: 64,
            iters: 1,
            lanes: 1,
        };
        let (mut a, mut b) = ([0u8; 32], [0u8; 32]);
        argon2id_bruto(b"x", &[7; 16], None, &p, &mut a).unwrap();
        argon2id_bruto(b"x", &[7; 16], None, &p, &mut b).unwrap();
        assert_eq!(a, b);
    }
}
