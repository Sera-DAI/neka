//! `neka ui`: tela interativa no estilo do Kotsu. Tabela de contas com a senha
//! oculta, tecla para revelar a selecionada, copiar, buscar. Só lê o cofre;
//! nada aqui grava. A lógica (`App`) não toca no terminal nem na área de
//! transferência: quem chama injeta a cópia, e os testes usam um buffer.

use std::time::{Duration, Instant};

use ratatui::crossterm::event::{self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;
use zeroize::Zeroizing;

use crate::ui::{self, Tom};

/// A senha volta a ficar oculta depois disto sem mexer em nada.
const REVELAR_SEGUNDOS: u64 = 15;
/// A área de transferência é limpa depois disto.
const LIMPAR_SEGUNDOS: u64 = 20;
const MASCARA: &str = "••••••••";

/// Uma conta na tela. A senha fica em memória que é zerada ao soltar.
pub struct Fila {
    pub id: String,
    pub titulo: String,
    pub usuario: String,
    pub url: String,
    pub notas: String,
    pub senha: Zeroizing<String>,
    pub data_senha: String,
    pub etiquetas: Vec<String>,
}

pub struct App {
    filas: Vec<Fila>,
    visiveis: Vec<usize>,
    sel: usize,
    revelada_em: Option<Instant>,
    busca: String,
    buscando: bool,
    aviso: Option<(String, Instant)>,
    copiado: Option<(Zeroizing<String>, Instant)>,
    pub sair: bool,
}

fn cor(t: Tom) -> Color {
    let (r, g, b) = ui::rgb(t);
    Color::Rgb(r, g, b)
}

fn fundo() -> Color {
    let ((r, g, b), _) = ui::fundos();
    Color::Rgb(r, g, b)
}

fn painel() -> Color {
    let (_, (r, g, b)) = ui::fundos();
    Color::Rgb(r, g, b)
}

fn estilo(t: Tom) -> Style {
    Style::new().fg(cor(t)).bg(fundo())
}

impl App {
    pub fn new(filas: Vec<Fila>) -> Self {
        let mut a = App {
            filas,
            visiveis: Vec::new(),
            sel: 0,
            revelada_em: None,
            busca: String::new(),
            buscando: false,
            aviso: None,
            copiado: None,
            sair: false,
        };
        a.filtrar();
        a
    }

    fn filtrar(&mut self) {
        let b = self.busca.to_lowercase();
        self.visiveis = (0..self.filas.len())
            .filter(|&i| {
                let f = &self.filas[i];
                b.is_empty()
                    || f.titulo.to_lowercase().contains(&b)
                    || f.usuario.to_lowercase().contains(&b)
                    || f.url.to_lowercase().contains(&b)
                    || f.etiquetas.iter().any(|t| t.to_lowercase().contains(&b))
            })
            .collect();
        self.sel = 0;
        self.revelada_em = None;
    }

    fn atual(&self) -> Option<&Fila> {
        self.visiveis.get(self.sel).map(|&i| &self.filas[i])
    }

    fn mover(&mut self, delta: isize) {
        if self.visiveis.is_empty() {
            return;
        }
        let max = self.visiveis.len() as isize - 1;
        self.sel = (self.sel as isize + delta).clamp(0, max) as usize;
        self.revelada_em = None;
    }

    fn dizer(&mut self, texto: impl Into<String>, agora: Instant) {
        self.aviso = Some((texto.into(), agora));
    }

    /// Trata uma tecla. `copiar` coloca texto na área de transferência.
    pub fn tecla(
        &mut self,
        k: KeyEvent,
        agora: Instant,
        copiar: &mut dyn FnMut(&str) -> Result<(), String>,
    ) {
        if k.kind != KeyEventKind::Press {
            return;
        }
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            self.sair = true;
            return;
        }
        if self.buscando {
            match k.code {
                KeyCode::Enter => self.buscando = false,
                KeyCode::Esc => {
                    self.buscando = false;
                    self.busca.clear();
                    self.filtrar();
                }
                KeyCode::Backspace => {
                    self.busca.pop();
                    self.filtrar();
                }
                KeyCode::Char(c) => {
                    self.busca.push(c);
                    self.filtrar();
                }
                _ => {}
            }
            return;
        }
        match k.code {
            KeyCode::Char('q') => self.sair = true,
            KeyCode::Esc => {
                if !self.busca.is_empty() {
                    self.busca.clear();
                    self.filtrar();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => self.mover(1),
            KeyCode::Up | KeyCode::Char('k') => self.mover(-1),
            KeyCode::PageDown => self.mover(10),
            KeyCode::PageUp => self.mover(-10),
            KeyCode::Home | KeyCode::Char('g') => self.mover(isize::MIN / 2),
            KeyCode::End | KeyCode::Char('G') => self.mover(isize::MAX / 2),
            KeyCode::Char('/') => self.buscando = true,
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('r') => {
                if self.atual().is_some() {
                    self.revelada_em = match self.revelada_em {
                        Some(_) => None,
                        None => Some(agora),
                    };
                }
            }
            KeyCode::Char(c @ ('c' | 'u')) => {
                let Some(f) = self.atual() else { return };
                let (texto, rotulo) = if c == 'c' {
                    (Zeroizing::new(f.senha.to_string()), "password")
                } else {
                    (Zeroizing::new(f.usuario.clone()), "username")
                };
                let titulo = f.titulo.clone();
                match copiar(&texto) {
                    Ok(()) => {
                        self.copiado = Some((texto, agora));
                        self.dizer(
                            format!(
                                "Copied the {rotulo} of {titulo}. Clearing in {LIMPAR_SEGUNDOS} s."
                            ),
                            agora,
                        );
                    }
                    Err(e) => self.dizer(e, agora),
                }
            }
            _ => {}
        }
    }

    /// Passa o tempo: esconde a senha revelada e limpa a área de transferência.
    pub fn tick(&mut self, agora: Instant, limpar: &mut dyn FnMut(&str)) {
        if let Some(t) = self.revelada_em {
            if agora.duration_since(t) >= Duration::from_secs(REVELAR_SEGUNDOS) {
                self.revelada_em = None;
            }
        }
        if let Some((texto, t)) = &self.copiado {
            if agora.duration_since(*t) >= Duration::from_secs(LIMPAR_SEGUNDOS) {
                limpar(texto);
                self.copiado = None;
                self.dizer("Clipboard cleared.", agora);
            }
        }
        if let Some((_, t)) = &self.aviso {
            if agora.duration_since(*t) >= Duration::from_secs(6) {
                self.aviso = None;
            }
        }
    }

    /// Ao sair, limpa a área de transferência se ainda houver o que copiamos.
    pub fn encerrar(&mut self, limpar: &mut dyn FnMut(&str)) {
        if let Some((texto, _)) = self.copiado.take() {
            limpar(&texto);
        }
    }

    pub fn desenhar(&self, f: &mut Frame) {
        let area = f.area();
        f.render_widget(Block::new().style(estilo(Tom::Texto)), area);
        let [topo, tabela, detalhe, estado, ajuda] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(5),
            Constraint::Length(1),
            Constraint::Length(1),
        ])
        .areas(area);

        self.desenhar_topo(f, topo);
        self.desenhar_tabela(f, tabela);
        self.desenhar_detalhe(f, detalhe);

        let linha_estado = if self.buscando || !self.busca.is_empty() {
            let cursor = if self.buscando { "▏" } else { "" };
            Line::from(vec![
                Span::styled(" / ", estilo(Tom::Destaque).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{}{cursor}", self.busca), estilo(Tom::Texto)),
            ])
        } else if let Some((m, _)) = &self.aviso {
            Line::from(Span::styled(format!(" ✧ {m}"), estilo(Tom::Aviso)))
        } else {
            Line::from("")
        };
        f.render_widget(
            Paragraph::new(linha_estado).style(estilo(Tom::Texto)),
            estado,
        );

        let tecla = |t: &str| {
            Span::styled(
                t.to_owned(),
                estilo(Tom::Secundaria).add_modifier(Modifier::BOLD),
            )
        };
        let txt = |t: &str| Span::styled(t.to_owned(), estilo(Tom::Neutro));
        let dicas = Line::from(vec![
            txt(" "),
            tecla("↑↓"),
            txt(" move  "),
            tecla("Enter"),
            txt(" reveal/hide  "),
            tecla("c"),
            txt(" copy password  "),
            tecla("u"),
            txt(" copy user  "),
            tecla("/"),
            txt(" search  "),
            tecla("q"),
            txt(" quit"),
        ]);
        f.render_widget(Paragraph::new(dicas).style(estilo(Tom::Texto)), ajuda);
    }

    fn desenhar_topo(&self, f: &mut Frame, area: Rect) {
        let esq = Line::from(Span::styled(
            " ☁ neka ☁",
            estilo(Tom::Destaque).add_modifier(Modifier::BOLD),
        ));
        let n = self.visiveis.len();
        let total = self.filas.len();
        let contagem = if n == 0 {
            "zzz… no entries ".to_owned()
        } else if n == total {
            format!("(◕‿◕)♡ {n} {} ", if n == 1 { "entry" } else { "entries" })
        } else {
            format!("(◕‿◕)♡ {n} of {total} ")
        };
        f.render_widget(Paragraph::new(esq).style(estilo(Tom::Texto)), area);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                contagem,
                estilo(if n == 0 { Tom::Neutro } else { Tom::Aviso }),
            )))
            .alignment(Alignment::Right)
            .style(estilo(Tom::Texto)),
            area,
        );
    }

    fn desenhar_tabela(&self, f: &mut Frame, area: Rect) {
        let bloco = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(estilo(Tom::Neutro))
            .style(estilo(Tom::Texto));
        let cab = ["Account", "User", "Password", "Set on", "Tags"]
            .map(|c| Cell::from(c).style(estilo(Tom::Secundaria).add_modifier(Modifier::BOLD)));
        let linhas: Vec<Row> = self
            .visiveis
            .iter()
            .enumerate()
            .map(|(pos, &i)| {
                let fl = &self.filas[i];
                let revelada = pos == self.sel && self.revelada_em.is_some();
                let senha = if revelada {
                    Cell::from(fl.senha.to_string())
                        .style(estilo(Tom::Aviso).add_modifier(Modifier::BOLD))
                } else if fl.senha.is_empty() {
                    Cell::from("(empty)").style(estilo(Tom::Neutro))
                } else {
                    Cell::from(MASCARA).style(estilo(Tom::Aviso))
                };
                Row::new(vec![
                    Cell::from(fl.titulo.clone())
                        .style(estilo(Tom::Texto).add_modifier(Modifier::BOLD)),
                    Cell::from(fl.usuario.clone()).style(estilo(Tom::Secundaria)),
                    senha,
                    Cell::from(fl.data_senha.clone()).style(estilo(Tom::Neutro)),
                    Cell::from(fl.etiquetas.join(", ")).style(estilo(Tom::Aviso)),
                ])
            })
            .collect();
        // A coluna cresce para caber a senha revelada inteira (nunca cortada).
        let largura_senha = match (self.revelada_em, self.atual()) {
            (Some(_), Some(f)) => f.senha.chars().count().max(MASCARA.len()) as u16,
            _ => 14,
        };
        let t = Table::new(
            linhas,
            [
                Constraint::Min(16),
                Constraint::Min(16),
                Constraint::Length(largura_senha),
                Constraint::Length(10),
                Constraint::Min(8),
            ],
        )
        .header(Row::new(cab).bottom_margin(1))
        .block(bloco)
        .column_spacing(2)
        .highlight_symbol("♡ ")
        .row_highlight_style(Style::new().bg(painel()).add_modifier(Modifier::BOLD));
        let mut estado = TableState::default();
        if !self.visiveis.is_empty() {
            estado.select(Some(self.sel));
        }
        f.render_stateful_widget(t, area, &mut estado);
    }

    fn desenhar_detalhe(&self, f: &mut Frame, area: Rect) {
        let bloco = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(estilo(Tom::Neutro))
            .style(estilo(Tom::Texto));
        let rot = |t: &str| {
            Span::styled(
                format!("{t:<7}"),
                estilo(Tom::Secundaria).add_modifier(Modifier::BOLD),
            )
        };
        let linhas = match self.atual() {
            Some(a) => {
                let notas = a.notas.lines().next().unwrap_or("").to_owned();
                vec![
                    Line::from(vec![
                        rot("Id"),
                        Span::styled(a.id.clone(), estilo(Tom::Neutro)),
                    ]),
                    Line::from(vec![
                        rot("URL"),
                        Span::styled(a.url.clone(), estilo(Tom::Texto)),
                    ]),
                    Line::from(vec![rot("Notes"), Span::styled(notas, estilo(Tom::Texto))]),
                ]
            }
            None => vec![Line::from(Span::styled(
                "Nothing to show.",
                estilo(Tom::Neutro),
            ))],
        };
        f.render_widget(Paragraph::new(linhas).block(bloco), area);
    }
}

/// Abre a tela e roda até o usuário sair.
pub fn executar(filas: Vec<Fila>) -> Result<(), String> {
    use std::cell::RefCell;
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        return Err("the interactive screen needs a terminal".into());
    }
    // Uma só área de transferência, usada para copiar e para limpar.
    let cb = RefCell::new(arboard::Clipboard::new().ok());
    let mut copiar = |t: &str| -> Result<(), String> {
        match cb.borrow_mut().as_mut() {
            Some(c) => crate::terminal::colocar(c, t),
            None => Err("clipboard unavailable".to_owned()),
        }
    };
    let mut limpar = |t: &str| {
        if let Some(c) = cb.borrow_mut().as_mut() {
            if c.get_text().ok().as_deref() == Some(t) {
                let _ = c.clear();
            }
        }
    };
    let mut app = App::new(filas);
    let mut term = ratatui::init();
    let r = laco(&mut term, &mut app, &mut copiar, &mut limpar);
    ratatui::restore();
    app.encerrar(&mut limpar);
    r.map_err(|_| "terminal error".to_owned())
}

fn laco(
    term: &mut ratatui::DefaultTerminal,
    app: &mut App,
    copiar: &mut dyn FnMut(&str) -> Result<(), String>,
    limpar: &mut dyn FnMut(&str),
) -> std::io::Result<()> {
    while !app.sair {
        term.draw(|f| app.desenhar(f))?;
        if event::poll(Duration::from_millis(250))? {
            if let Event::Key(k) = event::read()? {
                app.tecla(k, Instant::now(), copiar);
            }
        }
        app.tick(Instant::now(), limpar);
    }
    Ok(())
}

#[cfg(test)]
mod testes {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyEventState;
    use ratatui::Terminal;

    fn fila(titulo: &str, usuario: &str, senha: &str) -> Fila {
        Fila {
            id: "0123abcd".into(),
            titulo: titulo.into(),
            usuario: usuario.into(),
            url: format!("https://{titulo}.invalid"),
            notas: String::new(),
            senha: Zeroizing::new(senha.into()),
            data_senha: "2026-11-12".into(),
            etiquetas: vec!["tag".into()],
        }
    }

    fn app() -> App {
        App::new(vec![
            fila("Banco", "ana", "segredo-banco"),
            fila("Email", "bia", "segredo-email"),
            fila("Loja", "cris", "segredo-loja"),
        ])
    }

    fn k(c: KeyCode) -> KeyEvent {
        KeyEvent {
            code: c,
            modifiers: KeyModifiers::NONE,
            kind: KeyEventKind::Press,
            state: KeyEventState::NONE,
        }
    }

    fn tela(a: &App) -> String {
        let mut t = Terminal::new(TestBackend::new(100, 24)).unwrap();
        t.draw(|f| a.desenhar(f)).unwrap();
        let b = t.backend().buffer().clone();
        (0..b.area.height)
            .map(|y| {
                (0..b.area.width)
                    .map(|x| b[(x, y)].symbol())
                    .collect::<String>()
            })
            .collect::<Vec<_>>()
            .join("\n")
    }

    fn sem_copia() -> impl FnMut(&str) -> Result<(), String> {
        |_| Ok(())
    }

    #[test]
    fn senha_oculta_ate_revelar() {
        let mut a = app();
        let t0 = Instant::now();
        let s = tela(&a);
        assert!(s.contains("Banco") && s.contains(MASCARA));
        assert!(
            !s.contains("segredo-banco"),
            "senha não pode aparecer sem revelar"
        );
        a.tecla(k(KeyCode::Enter), t0, &mut sem_copia());
        assert!(tela(&a).contains("segredo-banco"));
        a.tecla(k(KeyCode::Enter), t0, &mut sem_copia());
        assert!(!tela(&a).contains("segredo-banco"));
    }

    #[test]
    fn mover_esconde_a_senha_e_revela_so_a_selecionada() {
        let mut a = app();
        let t0 = Instant::now();
        a.tecla(k(KeyCode::Enter), t0, &mut sem_copia());
        let s = tela(&a);
        assert!(s.contains("segredo-banco") && !s.contains("segredo-email"));
        a.tecla(k(KeyCode::Down), t0, &mut sem_copia());
        assert!(!tela(&a).contains("segredo-banco"));
    }

    #[test]
    fn senha_revelada_volta_a_ficar_oculta_com_o_tempo() {
        let mut a = app();
        let t0 = Instant::now();
        a.tecla(k(KeyCode::Enter), t0, &mut sem_copia());
        a.tick(t0 + Duration::from_secs(5), &mut |_| {});
        assert!(tela(&a).contains("segredo-banco"));
        a.tick(t0 + Duration::from_secs(REVELAR_SEGUNDOS + 1), &mut |_| {});
        assert!(!tela(&a).contains("segredo-banco"));
    }

    #[test]
    fn copiar_usa_a_senha_e_limpa_depois() {
        let mut a = app();
        let t0 = Instant::now();
        let mut copiado = Vec::new();
        a.tecla(k(KeyCode::Down), t0, &mut |t| {
            copiado.push(t.to_owned());
            Ok(())
        });
        a.tecla(k(KeyCode::Char('c')), t0, &mut |t| {
            copiado.push(t.to_owned());
            Ok(())
        });
        assert_eq!(copiado, ["segredo-email"]);
        assert!(!tela(&a).contains("segredo-email"), "copiar não revela");
        let mut limpo = Vec::new();
        a.tick(t0 + Duration::from_secs(LIMPAR_SEGUNDOS + 1), &mut |t| {
            limpo.push(t.to_owned())
        });
        assert_eq!(limpo, ["segredo-email"]);
    }

    #[test]
    fn busca_filtra_e_esc_limpa() {
        let mut a = app();
        let t0 = Instant::now();
        a.tecla(k(KeyCode::Char('/')), t0, &mut sem_copia());
        for c in "lo".chars() {
            a.tecla(k(KeyCode::Char(c)), t0, &mut sem_copia());
        }
        let s = tela(&a);
        assert!(s.contains("Loja") && !s.contains("Banco"), "{s}");
        a.tecla(k(KeyCode::Esc), t0, &mut sem_copia());
        assert!(tela(&a).contains("Banco"));
    }

    #[test]
    fn q_e_ctrl_c_saem_e_soltar_tecla_nao_conta() {
        let mut a = app();
        let t0 = Instant::now();
        let mut solta = k(KeyCode::Char('q'));
        solta.kind = KeyEventKind::Release;
        a.tecla(solta, t0, &mut sem_copia());
        assert!(!a.sair);
        a.tecla(k(KeyCode::Char('q')), t0, &mut sem_copia());
        assert!(a.sair);
        let mut b = app();
        let mut cc = k(KeyCode::Char('c'));
        cc.modifiers = KeyModifiers::CONTROL;
        b.tecla(cc, t0, &mut sem_copia());
        assert!(b.sair);
    }

    #[test]
    fn senha_longa_revelada_aparece_inteira() {
        let longa = "Xy9!".repeat(7);
        let mut a = App::new(vec![fila("Banco", "ana", &longa)]);
        a.tecla(k(KeyCode::Enter), Instant::now(), &mut sem_copia());
        assert!(tela(&a).contains(&longa));
    }

    #[test]
    fn lista_vazia_nao_quebra() {
        let mut a = App::new(Vec::new());
        let t0 = Instant::now();
        for c in [
            KeyCode::Down,
            KeyCode::Enter,
            KeyCode::Char('c'),
            KeyCode::End,
        ] {
            a.tecla(k(c), t0, &mut sem_copia());
        }
        assert!(tela(&a).contains("no entries"));
    }
}
