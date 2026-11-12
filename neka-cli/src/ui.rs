//! Visual do terminal, no estilo do Kotsu (tema "Algodão-Doce").
//!
//! Cores e enfeites (corações, nuvens, rostinhos) só aparecem quando o fluxo
//! é um terminal e `NO_COLOR` não está definido. Em pipe, redirecionamento ou
//! teste a saída é texto puro, sem glifos, para continuar fácil de filtrar
//! com `grep` e scripts. A paleta é a do Kotsu (noite por padrão; claro com
//! `NEKA_THEME=light`). Nenhum segredo passa por aqui além do que já seria
//! impresso: o estilo só envolve o texto, nunca o altera.

use std::fmt::Display;
use std::io::Write;
use std::sync::OnceLock;

use anstream::ColorChoice;
use clap::builder::styling::{Effects, RgbColor, Style, Styles};

#[derive(Clone, Copy)]
struct Paleta {
    destaque: RgbColor,
    secundaria: RgbColor,
    aviso: RgbColor,
    ok: RgbColor,
    erro: RgbColor,
    neutro: RgbColor,
    texto: RgbColor,
    fundo: RgbColor,
    painel: RgbColor,
}

/// Algodão-Doce noite (padrão).
const NOITE: Paleta = Paleta {
    destaque: RgbColor(0xff, 0x7f, 0xb8),
    secundaria: RgbColor(0x8f, 0xb3, 0xff),
    aviso: RgbColor(0xff, 0xd2, 0x7a),
    ok: RgbColor(0x86, 0xef, 0xc0),
    erro: RgbColor(0xff, 0x7a, 0x9e),
    neutro: RgbColor(0x9a, 0x7f, 0xa8),
    texto: RgbColor(0xff, 0xe9, 0xf4),
    fundo: RgbColor(0x1d, 0x14, 0x24),
    painel: RgbColor(0x41, 0x27, 0x4f),
};

/// Algodão-Doce claro, para terminais de fundo branco.
const CLARO: Paleta = Paleta {
    destaque: RgbColor(0xe0, 0x50, 0x9a),
    secundaria: RgbColor(0x4f, 0x74, 0xc7),
    aviso: RgbColor(0xb8, 0x74, 0x00),
    ok: RgbColor(0x1f, 0x9d, 0x6b),
    erro: RgbColor(0xd6, 0x33, 0x6c),
    neutro: RgbColor(0x8f, 0x6f, 0x8c),
    texto: RgbColor(0x5a, 0x3a, 0x5e),
    fundo: RgbColor(0xff, 0xf5, 0xfa),
    painel: RgbColor(0xff, 0xd6, 0xe8),
};

const MARCADOR: &str = "♡";
const NUVEM: &str = "☁";
const FAISCA: &str = "✧";
const ESTRELA: &str = "☆";
const FELIZ: &str = "(◕‿◕)♡";
const TRISTE: &str = "(；ω；)";
const ZERADO: &str = "zzz…";

/// Qual saída está sendo escrita.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Fluxo {
    Saida,
    Erro,
}

/// Papel da cor na paleta.
#[derive(Clone, Copy)]
pub enum Tom {
    Destaque,
    Secundaria,
    Aviso,
    Ok,
    Erro,
    Neutro,
    Texto,
}

fn paleta() -> &'static Paleta {
    static P: OnceLock<Paleta> = OnceLock::new();
    P.get_or_init(|| match std::env::var("NEKA_THEME") {
        Ok(v) if matches!(v.to_lowercase().as_str(), "light" | "claro") => CLARO,
        _ => NOITE,
    })
}

/// Cores e enfeites só em terminal, respeitando `NO_COLOR` e `TERM=dumb`.
pub fn enfeitado(f: Fluxo) -> bool {
    static SAIDA: OnceLock<bool> = OnceLock::new();
    static ERRO: OnceLock<bool> = OnceLock::new();
    match f {
        Fluxo::Saida => *SAIDA
            .get_or_init(|| anstream::AutoStream::choice(&std::io::stdout()) != ColorChoice::Never),
        Fluxo::Erro => *ERRO
            .get_or_init(|| anstream::AutoStream::choice(&std::io::stderr()) != ColorChoice::Never),
    }
}

fn cor(t: Tom) -> RgbColor {
    let p = paleta();
    match t {
        Tom::Destaque => p.destaque,
        Tom::Secundaria => p.secundaria,
        Tom::Aviso => p.aviso,
        Tom::Ok => p.ok,
        Tom::Erro => p.erro,
        Tom::Neutro => p.neutro,
        Tom::Texto => p.texto,
    }
}

fn estilo(t: Tom, negrito: bool) -> Style {
    let s = Style::new().fg_color(Some(cor(t).into()));
    if negrito {
        s.effects(Effects::BOLD)
    } else {
        s
    }
}

/// Texto na cor pedida; sem enfeite, devolve o texto como veio.
pub fn tom(f: Fluxo, t: Tom, negrito: bool, texto: impl Display) -> String {
    if enfeitado(f) {
        let e = estilo(t, negrito);
        format!("{e}{texto}{e:#}")
    } else {
        texto.to_string()
    }
}

/// Escreve uma linha (já estilizada) na saída pedida.
pub fn linha(f: Fluxo, texto: impl Display) {
    match f {
        Fluxo::Saida => anstream::println!("{texto}"),
        Fluxo::Erro => anstream::eprintln!("{texto}"),
    }
}

/// Mensagem de sucesso: `♡ texto` (só o coração ganha cor).
pub fn sucesso(f: Fluxo, texto: impl Display) {
    if enfeitado(f) {
        linha(f, format!("{} {texto}", tom(f, Tom::Ok, true, MARCADOR)));
    } else {
        linha(f, texto);
    }
}

/// Dica ou observação discreta: `☆ texto` em tom neutro.
pub fn dica(f: Fluxo, texto: impl Display) {
    if enfeitado(f) {
        linha(f, tom(f, Tom::Neutro, false, format!("{ESTRELA} {texto}")));
    } else {
        linha(f, texto);
    }
}

/// Aviso: `✧ warning: texto` em dourado.
pub fn aviso(texto: impl Display) {
    let f = Fluxo::Erro;
    if enfeitado(f) {
        linha(
            f,
            tom(f, Tom::Aviso, false, format!("{FAISCA} warning: {texto}")),
        );
    } else {
        linha(f, format!("warning: {texto}"));
    }
}

/// Erro: `(；ω；) error: texto` em rosa-avermelhado.
pub fn falha(texto: impl Display) {
    let f = Fluxo::Erro;
    if enfeitado(f) {
        linha(
            f,
            tom(f, Tom::Erro, true, format!("{TRISTE} error: {texto}")),
        );
    } else {
        linha(f, format!("error: {texto}"));
    }
}

/// Título de seção: `☁ texto ☁` (só em terminal).
pub fn titulo(f: Fluxo, texto: impl Display) {
    if enfeitado(f) {
        linha(
            f,
            tom(f, Tom::Destaque, true, format!("{NUVEM} {texto} {NUVEM}")),
        );
    }
}

/// Linha "Rótulo:  valor", com o rótulo alinhado em 10 colunas.
pub fn campo(rotulo: &str, valor: impl Display) {
    campo_largo(rotulo, 10, valor);
}

/// Como `campo`, com a largura do rótulo à escolha.
pub fn campo_largo(rotulo: &str, largura: usize, valor: impl Display) {
    let r = format!("{rotulo:<largura$}");
    linha(
        Fluxo::Saida,
        format!("{} {valor}", tom(Fluxo::Saida, Tom::Secundaria, true, r)),
    );
}

/// Trecho curto de texto de apoio (ids, endereços, datas).
pub fn apagado(texto: impl Display) -> String {
    tom(Fluxo::Saida, Tom::Neutro, false, texto)
}

/// Uma entrada na listagem: `♡ id  título  (usuário)  endereço  [etiquetas]`.
pub fn linha_entrada(id: &str, titulo: &str, usuario: &str, url: &str, etiquetas: &[String]) {
    let s = Fluxo::Saida;
    if !enfeitado(s) {
        let mut l = format!("{id}  {titulo}");
        if !usuario.is_empty() {
            l.push_str(&format!("  ({usuario})"));
        }
        if !url.is_empty() {
            l.push_str(&format!("  {url}"));
        }
        if !etiquetas.is_empty() {
            l.push_str(&format!("  [{}]", etiquetas.join(", ")));
        }
        linha(s, l);
        return;
    }
    let mut l = format!(
        "{} {}  {}",
        tom(s, Tom::Destaque, false, MARCADOR),
        tom(s, Tom::Neutro, false, id),
        tom(s, Tom::Texto, true, titulo)
    );
    if !usuario.is_empty() {
        l.push_str(&format!(
            "  {}",
            tom(s, Tom::Secundaria, false, format!("({usuario})"))
        ));
    }
    if !url.is_empty() {
        l.push_str(&format!("  {}", tom(s, Tom::Neutro, false, url)));
    }
    if !etiquetas.is_empty() {
        l.push_str(&format!(
            "  {}",
            tom(s, Tom::Aviso, false, format!("[{}]", etiquetas.join(", ")))
        ));
    }
    linha(s, l);
}

/// Rodapé da listagem: `(◕‿◕)♡ 3 entries`, ou `zzz… no entries` se vazia.
pub fn rodape_lista(n: usize) {
    let f = Fluxo::Erro;
    let quantas = match n {
        0 => "no entries".to_owned(),
        1 => "1 entry".to_owned(),
        n => format!("{n} entries"),
    };
    if !enfeitado(f) {
        linha(f, format!("{}.", capitalizar(&quantas)));
    } else if n == 0 {
        linha(f, tom(f, Tom::Neutro, false, format!("{ZERADO} {quantas}")));
    } else {
        linha(
            f,
            format!(
                "{} {}",
                tom(f, Tom::Aviso, true, FELIZ),
                tom(f, Tom::Neutro, false, quantas)
            ),
        );
    }
}

fn capitalizar(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(p) => p.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// Texto que antecede um pedido na entrada (prompt), com o coração em cor.
pub fn prompt_texto(texto: &str) -> String {
    let f = Fluxo::Erro;
    if enfeitado(f) {
        format!(
            "{} {}",
            tom(f, Tom::Destaque, true, MARCADOR),
            tom(f, Tom::Texto, false, texto)
        )
    } else {
        texto.to_owned()
    }
}

/// Escreve um prompt estilizado na saída de erro, sem quebra de linha.
pub fn prompt(texto: &str) {
    anstream::eprint!("{}", prompt_texto(texto));
    let _ = anstream::stderr().flush();
}

/// Bloco da chave de recuperação. A chave fica sozinha numa linha.
pub fn chave_de_recuperacao(chave: &str) {
    let s = Fluxo::Saida;
    linha(s, "");
    if enfeitado(s) {
        titulo(s, "RECOVERY KEY");
        linha(s, "");
        linha(s, format!("    {}", tom(s, Tom::Aviso, true, chave)));
        linha(s, "");
        linha(
            s,
            tom(
                s,
                Tom::Aviso,
                false,
                format!("{FAISCA} Shown only now. Write it on paper and keep it away from this computer."),
            ),
        );
    } else {
        linha(
            s,
            "RECOVERY KEY (shown only now; write it on paper and keep it away from this computer):",
        );
        linha(s, "");
        linha(s, format!("    {chave}"));
        linha(s, "");
    }
    linha(
        s,
        "Without it and the master password, nobody can recover the vault.",
    );
}

/// Cores do `--help`, na paleta do Kotsu.
pub fn estilos_clap() -> Styles {
    Styles::styled()
        .header(estilo(Tom::Destaque, true))
        .usage(estilo(Tom::Destaque, true))
        .literal(estilo(Tom::Secundaria, true))
        .placeholder(estilo(Tom::Aviso, false))
        .valid(estilo(Tom::Ok, false))
        .invalid(estilo(Tom::Aviso, false))
        .error(estilo(Tom::Erro, true))
}

/// Uma linha da tabela de entradas. A senha só vem preenchida com `--reveal`.
pub struct LinhaTabela {
    pub id: String,
    pub titulo: String,
    pub usuario: String,
    pub senha_vazia: bool,
    pub senha: Option<String>,
    pub data_senha: String,
    pub etiquetas: Vec<String>,
}

fn cortar(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// Tabela de entradas no estilo do Kotsu (só em terminal): conta, usuário,
/// senha oculta (ou revelada) e a data em que a senha foi definida.
pub fn tabela_entradas(linhas: &[LinhaTabela], revelar: bool) {
    let s = Fluxo::Saida;
    let cab = ["ID", "Account", "User", "Password", "Set on", "Tags"];
    let mut cel: Vec<[String; 6]> = Vec::new();
    for l in linhas {
        let senha = match (&l.senha, l.senha_vazia) {
            (Some(p), _) if revelar => p.clone(),
            (_, true) => "(empty)".to_owned(),
            _ => "••••••••".to_owned(),
        };
        cel.push([
            l.id.clone(),
            cortar(&l.titulo, 30),
            cortar(&l.usuario, 28),
            senha,
            l.data_senha.clone(),
            cortar(&l.etiquetas.join(", "), 24),
        ]);
    }
    let mut larg = cab.map(|c| c.chars().count());
    for linha in &cel {
        for (i, c) in linha.iter().enumerate() {
            larg[i] = larg[i].max(c.chars().count());
        }
    }
    let borda = |esq: &str, meio: &str, dir: &str| {
        let partes: Vec<String> = larg.iter().map(|w| "─".repeat(w + 2)).collect();
        tom(
            s,
            Tom::Neutro,
            false,
            format!("{esq}{}{dir}", partes.join(meio)),
        )
    };
    let v = tom(s, Tom::Neutro, false, "│");
    let montar = |valores: Vec<String>| {
        let partes: Vec<String> = valores.into_iter().map(|c| format!(" {c} ")).collect();
        format!("{v}{}{v}", partes.join(&v))
    };
    let pad = |c: &str, i: usize| format!("{c:<w$}", w = larg[i]);

    linha(s, borda("╭", "┬", "╮"));
    linha(
        s,
        montar(
            cab.iter()
                .enumerate()
                .map(|(i, c)| tom(s, Tom::Secundaria, true, pad(c, i)))
                .collect(),
        ),
    );
    linha(s, borda("├", "┼", "┤"));
    for l in &cel {
        let estilos = [
            (Tom::Neutro, false),
            (Tom::Texto, true),
            (Tom::Secundaria, false),
            (Tom::Aviso, revelar),
            (Tom::Neutro, false),
            (Tom::Aviso, false),
        ];
        linha(
            s,
            montar(
                l.iter()
                    .enumerate()
                    .map(|(i, c)| tom(s, estilos[i].0, estilos[i].1, pad(c, i)))
                    .collect(),
            ),
        );
    }
    linha(s, borda("╰", "┴", "╯"));
}

/// Cor da paleta como trio RGB, para telas que desenham por conta própria.
pub fn rgb(t: Tom) -> (u8, u8, u8) {
    let RgbColor(r, g, b) = cor(t);
    (r, g, b)
}

/// Fundo e painel (linha selecionada) da paleta, como trios RGB.
pub fn fundos() -> ((u8, u8, u8), (u8, u8, u8)) {
    let p = paleta();
    let RgbColor(a, b, c) = p.fundo;
    let RgbColor(d, e, f) = p.painel;
    ((a, b, c), (d, e, f))
}
