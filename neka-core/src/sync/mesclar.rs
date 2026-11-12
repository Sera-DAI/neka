use std::cmp::Ordering;
use std::collections::{BTreeMap, HashMap};

use crate::model::{Corpo, Entrada, Lapide, SenhaAntiga, Uuid, MAX_HISTORICO};

/// Lápides mais velhas que isto podem ser removidas (D12: 180 dias).
pub const RETENCAO_LAPIDES_MS: i64 = 180 * 24 * 60 * 60 * 1000;

/// Uma versão de um `id`: entrada viva ou lápide.
#[derive(Clone, Copy)]
enum Versao<'a> {
    Viva(&'a Entrada),
    Morta(&'a Lapide),
}

impl Versao<'_> {
    /// Tupla da D11: `(clock, updated_at ou deleted_at, device_id)`.
    fn chave(&self) -> (u64, i64, Uuid) {
        match self {
            Self::Viva(e) => (e.clock, e.updated_at, e.device_id),
            Self::Morta(l) => (l.clock, l.deleted_at, l.device_id),
        }
    }

    /// Ordem total: a tupla da D11 e, só para desempatar versões que
    /// divergem com a mesma tupla (não acontece com um dispositivo honesto),
    /// lápide antes de entrada e depois o conteúdo serializado.
    fn comparar(&self, outra: &Self) -> Ordering {
        self.chave()
            .cmp(&outra.chave())
            .then_with(|| match (self, outra) {
                (Self::Morta(a), Self::Morta(b)) => a.deleted_at.cmp(&b.deleted_at),
                (Self::Morta(_), Self::Viva(_)) => Ordering::Greater,
                (Self::Viva(_), Self::Morta(_)) => Ordering::Less,
                (Self::Viva(a), Self::Viva(b)) => bytes(a).cmp(&bytes(b)),
            })
    }
}

fn bytes(e: &Entrada) -> Vec<u8> {
    let mut v = Vec::new();
    // Serializar uma entrada válida não falha; se falhar, vale o vazio.
    let _ = ciborium::into_writer(e, &mut v);
    v
}

/// Junta históricos: guarda cada senha uma vez (com o instante mais recente
/// em que foi trocada) e fica com as [`MAX_HISTORICO`] mais recentes, da
/// mais antiga para a mais nova.
fn juntar_historico<'a>(itens: impl Iterator<Item = (&'a str, i64)>) -> Vec<SenhaAntiga> {
    let mut por_senha: HashMap<&str, i64> = HashMap::new();
    for (s, t) in itens {
        let atual = por_senha.entry(s).or_insert(t);
        if t > *atual {
            *atual = t;
        }
    }
    let mut v: Vec<(i64, &str)> = por_senha.into_iter().map(|(s, t)| (t, s)).collect();
    v.sort_unstable();
    let corte = v.len().saturating_sub(MAX_HISTORICO);
    v.drain(corte..)
        .map(|(t, s)| SenhaAntiga {
            password: s.to_owned(),
            changed_at: t,
        })
        .collect()
}

/// Mescla dois corpos do mesmo cofre, entrada por entrada (D11).
///
/// Para cada `id`, vence a versão de maior `(clock, data, device_id)`. Se a
/// vencedora é uma entrada, as senhas de todas as versões vivas entram no
/// histórico dela (nada se perde; a atual também fica registrada). Se é uma lápide, a entrada some.
/// O resultado sai ordenado por `id`, para não depender da ordem de entrada.
pub fn mesclar(a: &Corpo, b: &Corpo) -> Corpo {
    let mut versoes: BTreeMap<Uuid, Vec<Versao<'_>>> = BTreeMap::new();
    for c in [a, b] {
        for e in &c.entries {
            versoes.entry(e.id).or_default().push(Versao::Viva(e));
        }
        for l in &c.tombstones {
            versoes.entry(l.id).or_default().push(Versao::Morta(l));
        }
    }

    let mut entries = Vec::new();
    let mut tombstones = Vec::new();
    for (_, vs) in versoes {
        let Some(venc) = vs.iter().copied().max_by(|x, y| x.comparar(y)) else {
            continue;
        };
        match venc {
            Versao::Morta(l) => tombstones.push(l.clone()),
            Versao::Viva(e) => {
                let mut nova = e.clone();
                // Todas as versões vivas entram no histórico, inclusive a
                // vencedora: assim o resultado só depende do conjunto de
                // versões, e a mesclagem fica associativa. A interface
                // esconde do histórico a senha igual à atual.
                let mut itens: Vec<(&str, i64)> = Vec::new();
                for v in &vs {
                    if let Versao::Viva(x) = v {
                        itens.extend(
                            x.history
                                .iter()
                                .map(|h| (h.password.as_str(), h.changed_at)),
                        );
                        if !x.password.is_empty() {
                            itens.push((x.password.as_str(), x.updated_at));
                        }
                    }
                }
                nova.history = juntar_historico(itens.into_iter());
                entries.push(nova);
            }
        }
    }

    Corpo {
        schema: a.schema.max(b.schema),
        device_clock: a.device_clock.max(b.device_clock),
        entries,
        tombstones,
    }
}

/// Remove lápides com mais de [`RETENCAO_LAPIDES_MS`] (D12). Devolve
/// quantas saíram. Fica fora de [`mesclar`] para a mesclagem continuar pura.
pub fn podar_lapides(corpo: &mut Corpo, agora: i64) -> usize {
    let antes = corpo.tombstones.len();
    corpo
        .tombstones
        .retain(|l| agora.saturating_sub(l.deleted_at) <= RETENCAO_LAPIDES_MS);
    antes - corpo.tombstones.len()
}
