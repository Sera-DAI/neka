//! `neka ui`: interactive screen in the Kotsu style. Table of accounts with the password
//! hidden, a key to reveal the selected one, copy, search. Only reads the vault;
//! nothing here writes. The logic (`App`) does not touch the terminal or the
//! clipboard: the caller injects the copy, and tests use a buffer.

use std::cell::{Cell as CopyCell, RefCell};
use std::time::{Duration, Instant};

use ratatui::crossterm::event::{
    self, Event, KeyCode, KeyEvent, KeyEventKind, KeyModifiers, MouseButton, MouseEvent,
    MouseEventKind,
};
use ratatui::layout::{Alignment, Constraint, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Cell, Paragraph, Row, Table, TableState};
use ratatui::Frame;
use zeroize::Zeroizing;

use crate::forms::{Action, Confirmation, Data, Dialog, DialogKind, Form, Message, Padding};
use crate::ui::{self, Tone};

/// The password goes back to hidden after this, with no interaction.
const REVEAL_SECONDS: u64 = 15;
/// The clipboard is cleared after this.
const CLEAR_SECONDS: u64 = 20;
/// With no interaction for this long, the vault is locked and passwords leave memory.
const LOCK_SECONDS: u64 = 300;
/// Two clicks on the same row within this time count as a double click.
const DOUBLE_CLICK_MS: u128 = 500;
const MASK: &str = "••••••••";

/// An account on the screen. The password sits in memory that is zeroed on drop.
pub struct ListRow {
    pub id: String,
    pub title: String,
    pub username: String,
    pub url: String,
    pub notes: String,
    pub password: Zeroizing<String>,
    pub password_date: String,
    pub tags: Vec<String>,
}

/// What the screen needs from the vault. The caller implements it (the real app opens
/// the file; tests use an in-memory list). The screen never sees the vault.
pub trait Service {
    fn unlock(&mut self, password: &str) -> Result<Vec<ListRow>, String>;
    /// Adds (empty `id`) or edits; returns the new list and the affected id.
    fn save(&mut self, id: Option<&str>, d: Data) -> Result<(Vec<ListRow>, String), String>;
    fn remove(&mut self, id: &str) -> Result<Vec<ListRow>, String>;
    fn generate(&mut self, size: usize, symbols: bool) -> Result<String, String>;
    /// Releases the vault (and the file lock) when the screen locks.
    fn lock(&mut self);
    /// Syncs now with the configured folder.
    fn sync(&mut self) -> Result<Effect, String> {
        Err("not supported".into())
    }
    /// Runs a dialog: `v` are the values of the fields, in order.
    fn dialog(&mut self, _kind: DialogKind, _v: Vec<Zeroizing<String>>) -> Result<Effect, String> {
        Err("not supported".into())
    }
    /// Warning left over from an action (for example, an automatic sync
    /// failure after saving). Delivered only once.
    fn pending_warning(&mut self) -> Option<String> {
        None
    }
}

/// What a service action changes on the screen.
pub enum Effect {
    /// Just a short message.
    Warning(String),
    /// New list and short message.
    List(Vec<ListRow>, String),
    /// Recovery key to show (once). With a list, the vault has just
    /// been created and is open.
    Key(Zeroizing<String>, Option<Vec<ListRow>>),
}

/// Work the screen asks for and whoever runs the loop carries out.
pub enum Request {
    Generate {
        size: usize,
        symbols: bool,
    },
    Save {
        id: Option<String>,
        data: Data,
    },
    Remove {
        id: String,
    },
    Sync,
    Dialog {
        kind: DialogKind,
        values: Vec<Zeroizing<String>>,
    },
}

pub struct App {
    list_rows: Vec<ListRow>,
    visible: Vec<usize>,
    sel: usize,
    revealed_at: Option<Instant>,
    search: String,
    searching: bool,
    warning: Option<(String, Instant)>,
    copied: Option<(Zeroizing<String>, Instant)>,
    pub quit: bool,
    // Lock: with the vault locked there are no entries in memory.
    locked: bool,
    has_vault: bool,
    password_field: Zeroizing<String>,
    lock_error: Option<String>,
    busy: bool,
    /// Typed password, waiting for the caller to try opening the vault.
    pub password_request: Option<Zeroizing<String>>,
    lock_request: bool,
    form: Option<Form>,
    confirm: Option<Confirmation>,
    dialog: Option<Dialog>,
    message: Option<Message>,
    /// Generate password, save or remove, waiting for whoever runs the loop.
    pub request: Option<Request>,
    last_activity: Instant,
    // Geometry of the last drawn screen, for the mouse.
    table_state: RefCell<TableState>,
    table_area: CopyCell<Rect>,
    shortcuts: RefCell<Vec<(Rect, KeyCode)>>,
    last_click: Option<(usize, Instant)>,
}

fn color(t: Tone) -> Color {
    let (r, g, b) = ui::rgb(t);
    Color::Rgb(r, g, b)
}

fn background() -> Color {
    let ((r, g, b), _) = ui::backgrounds();
    Color::Rgb(r, g, b)
}

fn panel() -> Color {
    let (_, (r, g, b)) = ui::backgrounds();
    Color::Rgb(r, g, b)
}

pub(crate) fn style(t: Tone) -> Style {
    Style::new().fg(color(t)).bg(background())
}

impl App {
    pub fn new(list_rows: Vec<ListRow>) -> Self {
        let mut a = App {
            list_rows,
            visible: Vec::new(),
            sel: 0,
            revealed_at: None,
            search: String::new(),
            searching: false,
            warning: None,
            copied: None,
            quit: false,
            locked: false,
            has_vault: true,
            password_field: Zeroizing::new(String::new()),
            lock_error: None,
            busy: false,
            password_request: None,
            lock_request: false,
            form: None,
            confirm: None,
            dialog: None,
            message: None,
            request: None,
            last_activity: Instant::now(),
            table_state: RefCell::new(TableState::default()),
            table_area: CopyCell::new(Rect::default()),
            shortcuts: RefCell::new(Vec::new()),
            last_click: None,
        };
        a.apply_filter();
        a
    }

    /// App that starts locked: asks for the master password before showing anything.
    pub fn locked(has_vault: bool) -> Self {
        let mut a = App::new(Vec::new());
        a.locked = true;
        a.has_vault = has_vault;
        a
    }

    /// Result of the attempt to open the vault with `password_request`.
    pub fn unlock_result(&mut self, r: Result<Vec<ListRow>, String>, now: Instant) {
        self.busy = false;
        self.last_activity = now;
        match r {
            Ok(list_rows) => {
                self.list_rows = list_rows;
                self.locked = false;
                self.lock_error = None;
                self.search.clear();
                self.apply_filter();
            }
            Err(m) => self.lock_error = Some(m),
        }
    }

    fn lock(&mut self, clear: &mut dyn FnMut(&str)) {
        self.list_rows.clear();
        self.visible.clear();
        self.sel = 0;
        self.search.clear();
        self.searching = false;
        self.revealed_at = None;
        self.warning = None;
        self.lock_request = false;
        self.form = None;
        self.confirm = None;
        self.dialog = None;
        self.message = None;
        self.request = None;
        self.locked = true;
        self.password_field = Zeroizing::new(String::new());
        self.lock_error = None;
        if let Some((text, _)) = self.copied.take() {
            clear(&text);
        }
    }

    /// Result of sync, import, export, sync folder, changing the
    /// master password or creating the vault.
    pub fn action_result(&mut self, r: Result<Effect, String>, now: Instant) {
        match r {
            Err(m) => match &mut self.dialog {
                Some(d) => {
                    d.busy = false;
                    d.error = Some(m);
                }
                None => self.say(m, now),
            },
            Ok(effect) => {
                self.dialog = None;
                match effect {
                    Effect::Warning(m) => self.say(m, now),
                    Effect::List(list_rows, m) => {
                        self.swap_rows(list_rows, None);
                        self.say(m, now);
                    }
                    Effect::Key(text, list_rows) => {
                        if let Some(list_rows) = list_rows {
                            self.has_vault = true;
                            self.locked = false;
                            self.search.clear();
                            self.list_rows = list_rows;
                            self.apply_filter();
                        }
                        self.message = Some(Message::recovery_key(text));
                    }
                }
            }
        }
    }

    /// Joins an extra warning (from the service) with what is already on the screen.
    pub fn warn_extra(&mut self, extra: String, now: Instant) {
        let text = match &self.warning {
            Some((m, _)) => format!("{m} · {extra}"),
            None => extra,
        };
        self.say(text, now);
    }

    fn handle_dialog(&mut self, action: Action) {
        let Some(d) = &mut self.dialog else { return };
        match action {
            Action::Cancel => self.dialog = None,
            Action::Save => match d.validate() {
                Err(m) => d.error = Some(m),
                Ok(()) => {
                    d.error = None;
                    d.busy = true;
                    self.request = Some(Request::Dialog {
                        kind: d.kind,
                        values: d.values(),
                    });
                }
            },
            _ => {}
        }
    }

    pub fn is_locked(&self) -> bool {
        self.locked
    }

    /// Replaces the list (after save or remove), keeping the search and, if
    /// possible, the selection on the given `id` or at the same position.
    fn swap_rows(&mut self, list_rows: Vec<ListRow>, id: Option<&str>) {
        let before = self.sel;
        self.list_rows = list_rows;
        self.apply_filter();
        let target = id.and_then(|id| {
            self.visible
                .iter()
                .position(|&i| self.list_rows[i].id == id)
        });
        self.sel = target.unwrap_or_else(|| before.min(self.visible.len().saturating_sub(1)));
    }

    /// Result of saving: closes the form or shows the error in it.
    pub fn save_result(&mut self, r: Result<(Vec<ListRow>, String), String>, now: Instant) {
        match r {
            Ok((list_rows, id)) => {
                let new = self.form.as_ref().is_some_and(|f| f.id.is_none());
                self.form = None;
                self.swap_rows(list_rows, Some(&id));
                self.say(
                    if new {
                        "Account added."
                    } else {
                        "Account saved."
                    },
                    now,
                );
            }
            Err(m) => {
                if let Some(f) = &mut self.form {
                    f.busy = false;
                    f.error = Some(m);
                }
            }
        }
    }

    pub fn remove_result(&mut self, r: Result<Vec<ListRow>, String>, now: Instant) {
        self.confirm = None;
        match r {
            Ok(list_rows) => {
                self.swap_rows(list_rows, None);
                self.say("Account removed.", now);
            }
            Err(m) => self.say(m, now),
        }
    }

    pub fn generate_result(&mut self, r: Result<String, String>) {
        if let Some(f) = &mut self.form {
            match r {
                Ok(s) => f.generated_password(&s),
                Err(m) => {
                    f.busy = false;
                    f.error = Some(m);
                }
            }
        }
    }

    fn handle_form(&mut self, action: Action) {
        let Some(f) = &mut self.form else { return };
        match action {
            Action::Nothing => {}
            Action::Cancel => self.form = None,
            Action::Generate => {
                f.error = None;
                self.request = Some(Request::Generate {
                    size: f.size,
                    symbols: f.symbols,
                });
            }
            Action::Save => {
                if f.title_empty() {
                    f.error = Some("Title is required.".into());
                } else {
                    f.error = None;
                    f.busy = true;
                    self.request = Some(Request::Save {
                        id: f.id.clone(),
                        data: f.data(),
                    });
                }
            }
        }
    }

    fn handle_confirm(&mut self, r: Option<bool>) {
        match r {
            Some(true) => {
                if let Some(c) = &mut self.confirm {
                    c.busy = true;
                    self.request = Some(Request::Remove { id: c.id.clone() });
                }
            }
            Some(false) => self.confirm = None,
            None => {}
        }
    }

    fn open_form(&mut self, edit: bool) {
        if !edit {
            self.form = Some(Form::new());
            return;
        }
        if let Some(f) = self.current() {
            self.form = Some(Form::editing(Padding {
                id: &f.id,
                title: &f.title,
                username: &f.username,
                password: &f.password,
                url: &f.url,
                notes: &f.notes,
                tags: &f.tags,
            }));
            self.revealed_at = None;
        }
    }

    fn key_locked(&mut self, k: KeyEvent) {
        if self.busy {
            return;
        }
        match k.code {
            KeyCode::Esc => {
                if self.password_field.is_empty() {
                    self.quit = true;
                } else {
                    self.password_field = Zeroizing::new(String::new());
                }
            }
            KeyCode::Backspace => {
                self.password_field.pop();
            }
            KeyCode::Enter if !self.has_vault => {
                self.dialog = Some(Dialog::new(DialogKind::Create));
            }
            KeyCode::Enter => {
                if self.has_vault && !self.password_field.is_empty() {
                    let s =
                        std::mem::replace(&mut self.password_field, Zeroizing::new(String::new()));
                    self.password_request = Some(s);
                    self.lock_error = None;
                    self.busy = true;
                }
            }
            KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::CONTROL) => {
                self.password_field.push(c);
            }
            _ => {}
        }
    }

    /// Handles the mouse: wheel scrolls, click selects, double click reveals, and the
    /// footer shortcuts work like the keys they show.
    pub fn mouse(
        &mut self,
        m: MouseEvent,
        now: Instant,
        copy: &mut dyn FnMut(&str) -> Result<(), String>,
    ) {
        self.last_activity = now;
        if self.message.is_some() || self.dialog.is_some() {
            if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                if let Some(msg) = &self.message {
                    if msg.click(m.column, m.row) {
                        self.message = None;
                    }
                } else if let Some(d) = &mut self.dialog {
                    let action = d.click(m.column, m.row);
                    self.handle_dialog(action);
                }
            }
            return;
        }
        if self.locked {
            return;
        }
        if self.form.is_some() || self.confirm.is_some() {
            if let MouseEventKind::Down(MouseButton::Left) = m.kind {
                if let Some(f) = &mut self.form {
                    let action = f.click(m.column, m.row);
                    self.handle_form(action);
                } else if let Some(c) = &self.confirm {
                    let r = c.click(m.column, m.row);
                    self.handle_confirm(r);
                }
            }
            return;
        }
        if self.searching {
            return;
        }
        match m.kind {
            MouseEventKind::ScrollDown => self.shift(1),
            MouseEventKind::ScrollUp => self.shift(-1),
            MouseEventKind::Down(MouseButton::Left) => {
                let pos = ratatui::layout::Position::new(m.column, m.row);
                let shortcut = self
                    .shortcuts
                    .borrow()
                    .iter()
                    .find(|(r, _)| r.contains(pos))
                    .map(|(_, c)| *c);
                if let Some(code) = shortcut {
                    self.keypress(KeyEvent::new(code, KeyModifiers::NONE), now, copy);
                    return;
                }
                let area = self.table_area.get();
                // Top border, header and margin: the rows start 3 below.
                let first = area.y + 3;
                let last = (area.y + area.height).saturating_sub(2);
                if area.width == 0 || m.row < first || m.row > last {
                    return;
                }
                if m.column <= area.x || m.column >= area.x + area.width - 1 {
                    return;
                }
                let idx = self.table_state.borrow().offset() + (m.row - first) as usize;
                if idx >= self.visible.len() {
                    return;
                }
                let double = matches!(self.last_click, Some((i, t))
                    if i == idx && now.duration_since(t).as_millis() <= DOUBLE_CLICK_MS);
                if idx != self.sel {
                    self.sel = idx;
                    self.revealed_at = None;
                }
                if double {
                    self.revealed_at = match self.revealed_at {
                        Some(_) => None,
                        None => Some(now),
                    };
                    self.last_click = None;
                } else {
                    self.last_click = Some((idx, now));
                }
            }
            _ => {}
        }
    }

    fn apply_filter(&mut self) {
        let b = self.search.to_lowercase();
        self.visible = (0..self.list_rows.len())
            .filter(|&i| {
                let f = &self.list_rows[i];
                b.is_empty()
                    || f.title.to_lowercase().contains(&b)
                    || f.username.to_lowercase().contains(&b)
                    || f.url.to_lowercase().contains(&b)
                    || f.tags.iter().any(|t| t.to_lowercase().contains(&b))
            })
            .collect();
        self.sel = 0;
        self.revealed_at = None;
    }

    fn current(&self) -> Option<&ListRow> {
        self.visible.get(self.sel).map(|&i| &self.list_rows[i])
    }

    fn shift(&mut self, delta: isize) {
        if self.visible.is_empty() {
            return;
        }
        let max = self.visible.len() as isize - 1;
        self.sel = (self.sel as isize + delta).clamp(0, max) as usize;
        self.revealed_at = None;
    }

    fn say(&mut self, text: impl Into<String>, now: Instant) {
        self.warning = Some((text.into(), now));
    }

    /// Handles a key. `copy` puts text on the clipboard.
    pub fn keypress(
        &mut self,
        k: KeyEvent,
        now: Instant,
        copy: &mut dyn FnMut(&str) -> Result<(), String>,
    ) {
        if k.kind != KeyEventKind::Press {
            return;
        }
        self.last_activity = now;
        if k.modifiers.contains(KeyModifiers::CONTROL) && k.code == KeyCode::Char('c') {
            self.quit = true;
            return;
        }
        if let Some(m) = &self.message {
            if m.dismiss(k) {
                self.message = None;
            }
            return;
        }
        if let Some(d) = &mut self.dialog {
            let action = d.keypress(k);
            self.handle_dialog(action);
            return;
        }
        if self.locked {
            self.key_locked(k);
            return;
        }
        if let Some(f) = &mut self.form {
            let action = f.keypress(k);
            self.handle_form(action);
            return;
        }
        if let Some(c) = &self.confirm {
            let r = c.keypress(k);
            self.handle_confirm(r);
            return;
        }
        if self.searching {
            match k.code {
                KeyCode::Enter => self.searching = false,
                KeyCode::Esc => {
                    self.searching = false;
                    self.search.clear();
                    self.apply_filter();
                }
                KeyCode::Backspace => {
                    self.search.pop();
                    self.apply_filter();
                }
                KeyCode::Char(c) => {
                    self.search.push(c);
                    self.apply_filter();
                }
                _ => {}
            }
            return;
        }
        match k.code {
            KeyCode::Char('q') => self.quit = true,
            KeyCode::Char('l') => self.lock_request = true,
            KeyCode::Char('s') => {
                self.request = Some(Request::Sync);
                self.say("Syncing…", now);
            }
            KeyCode::Char('i') => self.dialog = Some(Dialog::new(DialogKind::Import)),
            KeyCode::Char('x') => self.dialog = Some(Dialog::new(DialogKind::Export)),
            KeyCode::Char('p') => self.dialog = Some(Dialog::new(DialogKind::MasterPassword)),
            KeyCode::Char('f') => self.dialog = Some(Dialog::new(DialogKind::SyncFolder)),
            KeyCode::Char('a') => self.open_form(false),
            KeyCode::Char('e') => self.open_form(true),
            KeyCode::Char('d') => {
                if let Some(f) = self.current() {
                    self.confirm = Some(Confirmation::new(f.id.clone(), f.title.clone()));
                    self.revealed_at = None;
                }
            }
            KeyCode::Esc => {
                if !self.search.is_empty() {
                    self.search.clear();
                    self.apply_filter();
                }
            }
            KeyCode::Down | KeyCode::Char('j') => self.shift(1),
            KeyCode::Up | KeyCode::Char('k') => self.shift(-1),
            KeyCode::PageDown => self.shift(10),
            KeyCode::PageUp => self.shift(-10),
            KeyCode::Home | KeyCode::Char('g') => self.shift(isize::MIN / 2),
            KeyCode::End | KeyCode::Char('G') => self.shift(isize::MAX / 2),
            KeyCode::Char('/') => self.searching = true,
            KeyCode::Enter | KeyCode::Char(' ') | KeyCode::Char('r') => {
                if self.current().is_some() {
                    self.revealed_at = match self.revealed_at {
                        Some(_) => None,
                        None => Some(now),
                    };
                }
            }
            KeyCode::Char(c @ ('c' | 'u')) => {
                let Some(f) = self.current() else { return };
                let (text, label) = if c == 'c' {
                    (Zeroizing::new(f.password.to_string()), "password")
                } else {
                    (Zeroizing::new(f.username.clone()), "username")
                };
                let title = f.title.clone();
                match copy(&text) {
                    Ok(()) => {
                        self.copied = Some((text, now));
                        self.say(
                            format!(
                                "Copied the {label} of {title}. Clearing in {CLEAR_SECONDS} s."
                            ),
                            now,
                        );
                    }
                    Err(e) => self.say(e, now),
                }
            }
            _ => {}
        }
    }

    /// Passes time: hides the revealed password and clears the clipboard.
    pub fn tick(&mut self, now: Instant, clear: &mut dyn FnMut(&str)) {
        if !self.locked
            && (self.lock_request
                || now.duration_since(self.last_activity) >= Duration::from_secs(LOCK_SECONDS))
        {
            self.lock(clear);
            return;
        }
        if let Some(t) = self.revealed_at {
            if now.duration_since(t) >= Duration::from_secs(REVEAL_SECONDS) {
                self.revealed_at = None;
            }
        }
        if let Some((text, t)) = &self.copied {
            if now.duration_since(*t) >= Duration::from_secs(CLEAR_SECONDS) {
                clear(text);
                self.copied = None;
                self.say("Clipboard cleared.", now);
            }
        }
        if let Some((_, t)) = &self.warning {
            if now.duration_since(*t) >= Duration::from_secs(6) {
                self.warning = None;
            }
        }
    }

    /// On exit, clears the clipboard if what we copied is still there.
    pub fn end(&mut self, clear: &mut dyn FnMut(&str)) {
        if let Some((text, _)) = self.copied.take() {
            clear(&text);
        }
    }

    pub fn draw(&self, f: &mut Frame) {
        let area = f.area();
        f.render_widget(Block::new().style(style(Tone::Text)), area);
        if self.locked {
            self.draw_lock(f, area);
            self.draw_windows(f, area);
            return;
        }
        let [top, table, detail, state, help] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(5),
            Constraint::Length(5),
            Constraint::Length(1),
            Constraint::Length(2),
        ])
        .areas(area);

        self.draw_top(f, top);
        self.draw_table(f, table);
        self.draw_detail(f, detail);

        let status_line = if self.searching || !self.search.is_empty() {
            let cursor = if self.searching { "▏" } else { "" };
            Line::from(vec![
                Span::styled(" / ", style(Tone::Highlight).add_modifier(Modifier::BOLD)),
                Span::styled(format!("{}{cursor}", self.search), style(Tone::Text)),
            ])
        } else if let Some((m, _)) = &self.warning {
            Line::from(Span::styled(format!(" ✧ {m}"), style(Tone::Warning)))
        } else {
            Line::from("")
        };
        f.render_widget(Paragraph::new(status_line).style(style(Tone::Text)), state);

        let lines: [Vec<(&str, &str, KeyCode)>; 2] = [
            vec![
                ("Enter", "show", KeyCode::Enter),
                ("c", "pass", KeyCode::Char('c')),
                ("u", "user", KeyCode::Char('u')),
                ("a", "add", KeyCode::Char('a')),
                ("e", "edit", KeyCode::Char('e')),
                ("d", "del", KeyCode::Char('d')),
                ("/", "find", KeyCode::Char('/')),
                ("l", "lock", KeyCode::Char('l')),
                ("q", "quit", KeyCode::Char('q')),
            ],
            vec![
                ("s", "sync", KeyCode::Char('s')),
                ("i", "import", KeyCode::Char('i')),
                ("x", "export", KeyCode::Char('x')),
                ("p", "password", KeyCode::Char('p')),
                ("f", "folder", KeyCode::Char('f')),
            ],
        ];
        let mut shortcuts = Vec::new();
        for (n, items) in lines.iter().enumerate() {
            let y = help.y + n as u16;
            let mut spans = vec![Span::styled(" ", style(Tone::Neutral))];
            let mut x = help.x + 1;
            for (keypress, label, code) in items {
                let width = (keypress.chars().count() + label.chars().count() + 3) as u16;
                if x + width - 1 <= help.x + help.width {
                    shortcuts.push((Rect::new(x, y, width - 1, 1), *code));
                }
                spans.push(Span::styled(
                    (*keypress).to_owned(),
                    style(Tone::Secondary).add_modifier(Modifier::BOLD),
                ));
                spans.push(Span::styled(format!(" {label}  "), style(Tone::Neutral)));
                x += width;
            }
            f.render_widget(
                Paragraph::new(Line::from(spans)).style(style(Tone::Text)),
                Rect::new(help.x, y, help.width, 1),
            );
        }
        *self.shortcuts.borrow_mut() = shortcuts;
        self.draw_windows(f, area);
    }

    /// Windows on top of the screen (form, confirmation, dialog, message).
    fn draw_windows(&self, f: &mut Frame, area: Rect) {
        if let Some(form) = &self.form {
            form.draw(f, area);
        } else if let Some(c) = &self.confirm {
            c.draw(f, area);
        }
        if let Some(d) = &self.dialog {
            d.draw(f, area);
        }
        if let Some(m) = &self.message {
            m.draw(f, area);
        }
    }

    fn draw_lock(&self, f: &mut Frame, area: Rect) {
        let popup = Rect {
            x: area.x + area.width.saturating_sub(46) / 2,
            y: area.y + area.height.saturating_sub(9) / 2,
            width: area.width.min(46),
            height: area.height.min(9),
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(style(Tone::Highlight))
            .title(Span::styled(
                " ☁ neka ☁ ",
                style(Tone::Highlight).add_modifier(Modifier::BOLD),
            ))
            .style(style(Tone::Text));
        let lines = if !self.has_vault {
            vec![
                Line::from(Span::styled(
                    "No vault on this device yet.",
                    style(Tone::Warning),
                )),
                Line::from(""),
                Line::from(Span::styled(
                    "Press Enter to create it.",
                    style(Tone::Secondary).add_modifier(Modifier::BOLD),
                )),
                Line::from(Span::styled(
                    "(Have a vault in the cloud? Use `neka clone <file>`.)",
                    style(Tone::Neutral),
                )),
            ]
        } else {
            let field = "•".repeat(self.password_field.chars().count());
            let cursor = if self.busy { "" } else { "▏" };
            let state = if self.busy {
                Span::styled("Unlocking…", style(Tone::Warning))
            } else if let Some(m) = &self.lock_error {
                Span::styled(format!("✧ {m}"), style(Tone::Warning))
            } else {
                Span::styled("", style(Tone::Text))
            };
            vec![
                Line::from(Span::styled("Master password", style(Tone::Secondary))),
                Line::from(Span::styled(
                    format!("{field}{cursor}"),
                    style(Tone::Text).add_modifier(Modifier::BOLD),
                )),
                Line::from(""),
                Line::from(state),
                Line::from(""),
                Line::from(vec![
                    Span::styled("Enter", style(Tone::Secondary).add_modifier(Modifier::BOLD)),
                    Span::styled(" unlock  ", style(Tone::Neutral)),
                    Span::styled("Esc", style(Tone::Secondary).add_modifier(Modifier::BOLD)),
                    Span::styled(" clear/quit", style(Tone::Neutral)),
                ]),
            ]
        };
        f.render_widget(
            Paragraph::new(lines)
                .alignment(Alignment::Center)
                .block(block),
            popup,
        );
    }

    fn draw_top(&self, f: &mut Frame, area: Rect) {
        let left = Line::from(Span::styled(
            " ☁ neka ☁",
            style(Tone::Highlight).add_modifier(Modifier::BOLD),
        ));
        let n = self.visible.len();
        let total = self.list_rows.len();
        let count = if n == 0 {
            "zzz… no entries ".to_owned()
        } else if n == total {
            format!("(◕‿◕)♡ {n} {} ", if n == 1 { "entry" } else { "entries" })
        } else {
            format!("(◕‿◕)♡ {n} of {total} ")
        };
        f.render_widget(Paragraph::new(left).style(style(Tone::Text)), area);
        f.render_widget(
            Paragraph::new(Line::from(Span::styled(
                count,
                style(if n == 0 { Tone::Neutral } else { Tone::Warning }),
            )))
            .alignment(Alignment::Right)
            .style(style(Tone::Text)),
            area,
        );
    }

    fn draw_table(&self, f: &mut Frame, area: Rect) {
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(style(Tone::Neutral))
            .style(style(Tone::Text));
        let header = ["Account", "User", "Password", "Set on", "Tags"]
            .map(|c| Cell::from(c).style(style(Tone::Secondary).add_modifier(Modifier::BOLD)));
        let lines: Vec<Row> = self
            .visible
            .iter()
            .enumerate()
            .map(|(pos, &i)| {
                let fl = &self.list_rows[i];
                let revealed = pos == self.sel && self.revealed_at.is_some();
                let password = if revealed {
                    Cell::from(fl.password.to_string())
                        .style(style(Tone::Warning).add_modifier(Modifier::BOLD))
                } else if fl.password.is_empty() {
                    Cell::from("(empty)").style(style(Tone::Neutral))
                } else {
                    Cell::from(MASK).style(style(Tone::Warning))
                };
                Row::new(vec![
                    Cell::from(fl.title.clone())
                        .style(style(Tone::Text).add_modifier(Modifier::BOLD)),
                    Cell::from(fl.username.clone()).style(style(Tone::Secondary)),
                    password,
                    Cell::from(fl.password_date.clone()).style(style(Tone::Neutral)),
                    Cell::from(fl.tags.join(", ")).style(style(Tone::Warning)),
                ])
            })
            .collect();
        // The column grows to fit the whole revealed password (never cut off).
        let password_width = match (self.revealed_at, self.current()) {
            (Some(_), Some(f)) => f.password.chars().count().max(MASK.len()) as u16,
            _ => 14,
        };
        let t = Table::new(
            lines,
            [
                Constraint::Min(16),
                Constraint::Min(16),
                Constraint::Length(password_width),
                Constraint::Length(10),
                Constraint::Min(8),
            ],
        )
        .header(Row::new(header).bottom_margin(1))
        .block(block)
        .column_spacing(2)
        .highlight_symbol("♡ ")
        .row_highlight_style(Style::new().bg(panel()).add_modifier(Modifier::BOLD));
        let mut state = self.table_state.borrow_mut();
        state.select(if self.visible.is_empty() {
            None
        } else {
            Some(self.sel)
        });
        f.render_stateful_widget(t, area, &mut *state);
        self.table_area.set(area);
    }

    fn draw_detail(&self, f: &mut Frame, area: Rect) {
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(style(Tone::Neutral))
            .style(style(Tone::Text));
        let rot = |t: &str| {
            Span::styled(
                format!("{t:<7}"),
                style(Tone::Secondary).add_modifier(Modifier::BOLD),
            )
        };
        let lines = match self.current() {
            Some(a) => {
                let notes = a.notes.lines().next().unwrap_or("").to_owned();
                vec![
                    Line::from(vec![
                        rot("Id"),
                        Span::styled(a.id.clone(), style(Tone::Neutral)),
                    ]),
                    Line::from(vec![
                        rot("URL"),
                        Span::styled(a.url.clone(), style(Tone::Text)),
                    ]),
                    Line::from(vec![rot("Notes"), Span::styled(notes, style(Tone::Text))]),
                ]
            }
            None => vec![Line::from(Span::styled(
                "Nothing to show.",
                style(Tone::Neutral),
            ))],
        };
        f.render_widget(Paragraph::new(lines).block(block), area);
    }
}

/// Runs what the screen asked for and hands the result back to it.
pub fn serve(app: &mut App, service: &mut dyn Service, now: Instant) {
    if let Some(password) = app.password_request.take() {
        let r = service.unlock(&password);
        app.unlock_result(r, now);
    }
    match app.request.take() {
        None => {}
        Some(Request::Generate { size, symbols }) => {
            let r = service.generate(size, symbols);
            app.generate_result(r);
        }
        Some(Request::Save { id, data }) => {
            let r = service.save(id.as_deref(), data);
            app.save_result(r, now);
        }
        Some(Request::Remove { id }) => {
            let r = service.remove(&id);
            app.remove_result(r, now);
        }
        Some(Request::Sync) => {
            let r = service.sync();
            app.action_result(r, now);
        }
        Some(Request::Dialog { kind, values }) => {
            let r = service.dialog(kind, values);
            app.action_result(r, now);
        }
    }
    if let Some(extra) = service.pending_warning() {
        app.warn_extra(extra, now);
    }
    if app.is_locked() {
        service.lock();
    }
}

/// Opens the (locked) screen and runs until the user quits.
pub fn run(has_vault: bool, service: &mut dyn Service) -> Result<(), String> {
    use ratatui::crossterm::event::{DisableMouseCapture, EnableMouseCapture};
    use ratatui::crossterm::execute;
    use std::io::IsTerminal;
    if !std::io::stdout().is_terminal() || !std::io::stdin().is_terminal() {
        return Err("the interactive screen needs a terminal; see `neka --help`".into());
    }
    // A single clipboard, used to copy and to clear.
    let cb = RefCell::new(arboard::Clipboard::new().ok());
    let mut copy = |t: &str| -> Result<(), String> {
        match cb.borrow_mut().as_mut() {
            Some(c) => crate::terminal::put(c, t),
            None => Err("clipboard unavailable".to_owned()),
        }
    };
    let mut clear = |t: &str| {
        if let Some(c) = cb.borrow_mut().as_mut() {
            if c.get_text().ok().as_deref() == Some(t) {
                let _ = c.clear();
            }
        }
    };
    let mut app = App::locked(has_vault);
    let mut term = ratatui::init();
    // If something panics, the terminal must not stay with the mouse captured.
    let hook = std::panic::take_hook();
    std::panic::set_hook(Box::new(move |info| {
        let _ = execute!(std::io::stdout(), DisableMouseCapture);
        hook(info);
    }));
    let _ = execute!(std::io::stdout(), EnableMouseCapture);
    let r = event_loop(&mut term, &mut app, &mut copy, &mut clear, service);
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    app.end(&mut clear);
    service.lock();
    r.map_err(|_| "terminal error".to_owned())
}

fn event_loop(
    term: &mut ratatui::DefaultTerminal,
    app: &mut App,
    copy: &mut dyn FnMut(&str) -> Result<(), String>,
    clear: &mut dyn FnMut(&str),
    service: &mut dyn Service,
) -> std::io::Result<()> {
    while !app.quit {
        term.draw(|f| app.draw(f))?;
        if event::poll(Duration::from_millis(250))? {
            match event::read()? {
                Event::Key(k) => app.keypress(k, Instant::now(), copy),
                Event::Mouse(m) => app.mouse(m, Instant::now(), copy),
                _ => {}
            }
        }
        if app.password_request.is_some() || app.request.is_some() {
            // Deriving the key takes about a second: draws the notice first.
            term.draw(|f| app.draw(f))?;
            serve(app, service, Instant::now());
        }
        app.tick(Instant::now(), clear);
        if app.is_locked() {
            service.lock();
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use ratatui::backend::TestBackend;
    use ratatui::crossterm::event::KeyEventState;
    use ratatui::Terminal;

    fn list_row(title: &str, username: &str, password: &str) -> ListRow {
        ListRow {
            id: "0123abcd".into(),
            title: title.into(),
            username: username.into(),
            url: format!("https://{title}.invalid"),
            notes: String::new(),
            password: Zeroizing::new(password.into()),
            password_date: "2026-11-12".into(),
            tags: vec!["tag".into()],
        }
    }

    fn app() -> App {
        App::new(vec![
            list_row("Bank", "ana", "secret-bank"),
            list_row("Email", "bia", "secret-email"),
            list_row("Store", "cris", "secret-store"),
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

    fn screen(a: &App) -> String {
        let mut t = Terminal::new(TestBackend::new(100, 24)).unwrap();
        t.draw(|f| a.draw(f)).unwrap();
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

    fn no_copy() -> impl FnMut(&str) -> Result<(), String> {
        |_| Ok(())
    }

    #[test]
    fn password_hidden_until_revealed() {
        let mut a = app();
        let t0 = Instant::now();
        let s = screen(&a);
        assert!(s.contains("Bank") && s.contains(MASK));
        assert!(
            !s.contains("secret-bank"),
            "password must not appear without reveal"
        );
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        assert!(screen(&a).contains("secret-bank"));
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        assert!(!screen(&a).contains("secret-bank"));
    }

    #[test]
    fn moving_hides_the_password_and_reveals_only_the_selected() {
        let mut a = app();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        let s = screen(&a);
        assert!(s.contains("secret-bank") && !s.contains("secret-email"));
        a.keypress(k(KeyCode::Down), t0, &mut no_copy());
        assert!(!screen(&a).contains("secret-bank"));
    }

    #[test]
    fn revealed_password_hides_again_over_time() {
        let mut a = app();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        a.tick(t0 + Duration::from_secs(5), &mut |_| {});
        assert!(screen(&a).contains("secret-bank"));
        a.tick(t0 + Duration::from_secs(REVEAL_SECONDS + 1), &mut |_| {});
        assert!(!screen(&a).contains("secret-bank"));
    }

    #[test]
    fn copy_uses_the_password_and_clears_afterwards() {
        let mut a = app();
        let t0 = Instant::now();
        let mut copied = Vec::new();
        a.keypress(k(KeyCode::Down), t0, &mut |t| {
            copied.push(t.to_owned());
            Ok(())
        });
        a.keypress(k(KeyCode::Char('c')), t0, &mut |t| {
            copied.push(t.to_owned());
            Ok(())
        });
        assert_eq!(copied, ["secret-email"]);
        assert!(!screen(&a).contains("secret-email"), "copy does not reveal");
        let mut limpo = Vec::new();
        a.tick(t0 + Duration::from_secs(CLEAR_SECONDS + 1), &mut |t| {
            limpo.push(t.to_owned())
        });
        assert_eq!(limpo, ["secret-email"]);
    }

    #[test]
    fn search_filters_and_esc_clears() {
        let mut a = app();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('/')), t0, &mut no_copy());
        for c in "sto".chars() {
            a.keypress(k(KeyCode::Char(c)), t0, &mut no_copy());
        }
        let s = screen(&a);
        assert!(s.contains("Store") && !s.contains("Bank"), "{s}");
        a.keypress(k(KeyCode::Esc), t0, &mut no_copy());
        assert!(screen(&a).contains("Bank"));
    }

    #[test]
    fn q_and_ctrl_c_quit_and_key_release_does_not_count() {
        let mut a = app();
        let t0 = Instant::now();
        let mut solta = k(KeyCode::Char('q'));
        solta.kind = KeyEventKind::Release;
        a.keypress(solta, t0, &mut no_copy());
        assert!(!a.quit);
        a.keypress(k(KeyCode::Char('q')), t0, &mut no_copy());
        assert!(a.quit);
        let mut b = app();
        let mut cc = k(KeyCode::Char('c'));
        cc.modifiers = KeyModifiers::CONTROL;
        b.keypress(cc, t0, &mut no_copy());
        assert!(b.quit);
    }

    #[test]
    fn long_revealed_password_appears_in_full() {
        let long = "Xy9!".repeat(7);
        let mut a = App::new(vec![list_row("Bank", "ana", &long)]);
        a.keypress(k(KeyCode::Enter), Instant::now(), &mut no_copy());
        assert!(screen(&a).contains(&long));
    }

    #[test]
    fn empty_list_does_not_break() {
        let mut a = App::new(Vec::new());
        let t0 = Instant::now();
        for c in [
            KeyCode::Down,
            KeyCode::Enter,
            KeyCode::Char('c'),
            KeyCode::End,
        ] {
            a.keypress(k(c), t0, &mut no_copy());
        }
        assert!(screen(&a).contains("no entries"));
    }

    fn m(kind: MouseEventKind, x: u16, y: u16) -> MouseEvent {
        MouseEvent {
            kind,
            column: x,
            row: y,
            modifiers: KeyModifiers::NONE,
        }
    }

    /// Position (x, y) where the text appears on the test screen.
    fn find_text(a: &App, text: &str) -> (u16, u16) {
        let s = screen(a);
        for (y, l) in s.lines().enumerate() {
            if let Some(i) = l.find(text) {
                return (l[..i].chars().count() as u16, y as u16);
            }
        }
        panic!("`{text}` is not on the screen:\n{s}");
    }

    #[test]
    fn mouse_wheel_moves_the_selection() {
        let mut a = app();
        let t0 = Instant::now();
        let _ = screen(&a);
        a.mouse(m(MouseEventKind::ScrollDown, 5, 5), t0, &mut no_copy());
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        assert!(screen(&a).contains("secret-email"));
        a.mouse(m(MouseEventKind::ScrollUp, 5, 5), t0, &mut no_copy());
        assert!(!screen(&a).contains("secret-email"));
    }

    #[test]
    fn click_selects_and_double_click_reveals() {
        let mut a = app();
        let t0 = Instant::now();
        let (x, y) = find_text(&a, "Email");
        let click = m(MouseEventKind::Down(MouseButton::Left), x, y);
        a.mouse(click, t0, &mut no_copy());
        assert!(
            !screen(&a).contains("secret-email"),
            "a single click does not reveal"
        );
        a.mouse(click, t0 + Duration::from_millis(200), &mut no_copy());
        let s = screen(&a);
        assert!(s.contains("secret-email") && !s.contains("secret-bank"));
    }

    #[test]
    fn two_slow_clicks_do_not_reveal() {
        let mut a = app();
        let t0 = Instant::now();
        let (x, y) = find_text(&a, "Email");
        let click = m(MouseEventKind::Down(MouseButton::Left), x, y);
        a.mouse(click, t0, &mut no_copy());
        a.mouse(click, t0 + Duration::from_secs(2), &mut no_copy());
        assert!(!screen(&a).contains("secret-email"));
    }

    #[test]
    fn click_outside_the_rows_does_not_break() {
        let mut a = app();
        let t0 = Instant::now();
        let _ = screen(&a);
        for (x, y) in [(0, 0), (99, 0), (5, 15), (200, 200)] {
            a.mouse(
                m(MouseEventKind::Down(MouseButton::Left), x, y),
                t0,
                &mut no_copy(),
            );
        }
        assert!(!a.quit);
    }

    #[test]
    fn clicking_a_footer_shortcut_does_the_same_as_the_key() {
        let mut a = app();
        let t0 = Instant::now();
        let mut copied = Vec::new();
        let (x, y) = find_text(&a, "c pass");
        a.mouse(
            m(MouseEventKind::Down(MouseButton::Left), x, y),
            t0,
            &mut |t| {
                copied.push(t.to_owned());
                Ok(())
            },
        );
        assert_eq!(copied, ["secret-bank"]);
        let (x, y) = find_text(&a, "quit");
        a.mouse(
            m(MouseEventKind::Down(MouseButton::Left), x, y),
            t0,
            &mut no_copy(),
        );
        assert!(a.quit);
    }

    fn type_text(a: &mut App, text: &str, t: Instant) {
        for c in text.chars() {
            a.keypress(k(KeyCode::Char(c)), t, &mut no_copy());
        }
    }

    #[test]
    fn locked_shows_nothing_and_the_password_does_not_appear() {
        let mut a = App::locked(true);
        let t0 = Instant::now();
        type_text(&mut a, "my-password", t0);
        let s = screen(&a);
        assert!(s.contains("Master password") && !s.contains("my-password"));
        assert!(s.contains("•••••••••••"));
        // `q` is text here, it does not quit.
        assert!(!a.quit);
    }

    #[test]
    fn enter_asks_for_unlock_and_an_error_keeps_it_locked() {
        let mut a = App::locked(true);
        let t0 = Instant::now();
        type_text(&mut a, "wrong", t0);
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        let request = a.password_request.take().expect("asked to open");
        assert_eq!(request.as_str(), "wrong");
        a.unlock_result(Err("incorrect password".into()), t0);
        let s = screen(&a);
        assert!(s.contains("incorrect password") && !s.contains("Bank"));
        // Right one: opens the list.
        type_text(&mut a, "right", t0);
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        assert!(a.password_request.take().is_some());
        a.unlock_result(Ok(vec![list_row("Bank", "ana", "secret-bank")]), t0);
        let s = screen(&a);
        assert!(s.contains("Bank") && !s.contains("secret-bank"));
    }

    #[test]
    fn empty_password_does_not_try_to_open_and_esc_clears_then_quits() {
        let mut a = App::locked(true);
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        assert!(a.password_request.is_none());
        type_text(&mut a, "abc", t0);
        a.keypress(k(KeyCode::Esc), t0, &mut no_copy());
        assert!(!a.quit);
        a.keypress(k(KeyCode::Esc), t0, &mut no_copy());
        assert!(a.quit);
    }

    #[test]
    fn without_a_vault_it_explains_and_does_not_ask_for_a_password() {
        let mut a = App::locked(false);
        let t0 = Instant::now();
        type_text(&mut a, "x", t0);
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        assert!(a.password_request.is_none());
        assert!(screen(&a).contains("Create your vault"));
    }

    #[test]
    fn key_l_locks_and_clears_passwords_and_clipboard() {
        let mut a = app();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('c')), t0, &mut no_copy());
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        a.keypress(k(KeyCode::Char('l')), t0, &mut no_copy());
        let mut limpo = Vec::new();
        a.tick(t0, &mut |t| limpo.push(t.to_owned()));
        assert_eq!(limpo, ["secret-bank"]);
        let s = screen(&a);
        assert!(s.contains("Master password") && !s.contains("Bank"));
        assert!(a.list_rows.is_empty(), "entries leave memory on lock");
    }

    #[test]
    fn inactivity_locks_and_activity_postpones() {
        let mut a = app();
        let t0 = Instant::now();
        a.tick(t0 + Duration::from_secs(LOCK_SECONDS - 1), &mut |_| {});
        assert!(!a.locked);
        // Moving the mouse counts as activity.
        a.mouse(
            m(MouseEventKind::ScrollDown, 1, 1),
            t0 + Duration::from_secs(LOCK_SECONDS - 1),
            &mut no_copy(),
        );
        a.tick(t0 + Duration::from_secs(LOCK_SECONDS + 5), &mut |_| {});
        assert!(!a.locked);
        a.tick(t0 + Duration::from_secs(2 * LOCK_SECONDS), &mut |_| {});
        assert!(a.locked);
    }

    // --- add, edit, remove and generate -------------------------------------

    struct Fake {
        list_rows: Vec<(String, String, String)>, // id, title, password
        notes: String,
        last: Option<(Option<String>, String, String)>, // id, title, notes
        failure: Option<String>,
        locked: usize,
        response: Option<Result<Effect, String>>,
        calls: Vec<(String, Vec<String>)>,
        extra: Option<String>,
    }

    impl Fake {
        fn new() -> Self {
            Fake {
                list_rows: vec![
                    ("id-bank".into(), "Bank".into(), "secret-bank".into()),
                    ("id-email".into(), "Email".into(), "secret-email".into()),
                ],
                notes: String::new(),
                last: None,
                failure: None,
                locked: 0,
                response: None,
                calls: Vec::new(),
                extra: None,
            }
        }

        fn list(&self) -> Vec<ListRow> {
            let mut v: Vec<_> = self.list_rows.clone();
            v.sort_by_key(|f| f.1.to_lowercase());
            v.iter()
                .map(|(id, t, s)| {
                    let mut f = list_row(t, "ana", s);
                    f.id = id.clone();
                    f.notes = self.notes.clone();
                    f
                })
                .collect()
        }
    }

    impl Service for Fake {
        fn unlock(&mut self, _: &str) -> Result<Vec<ListRow>, String> {
            Ok(self.list())
        }
        fn save(&mut self, id: Option<&str>, d: Data) -> Result<(Vec<ListRow>, String), String> {
            if let Some(m) = &self.failure {
                return Err(m.clone());
            }
            self.last = Some((
                id.map(str::to_owned),
                d.title.to_string(),
                d.notes.to_string(),
            ));
            let id = match id {
                Some(id) => {
                    let f = self.list_rows.iter_mut().find(|f| f.0 == id).unwrap();
                    f.1 = d.title.to_string();
                    f.2 = d.password.to_string();
                    id.to_owned()
                }
                None => {
                    let id = format!("id-new{}", self.list_rows.len());
                    self.list_rows
                        .push((id.clone(), d.title.to_string(), d.password.to_string()));
                    id
                }
            };
            Ok((self.list(), id))
        }
        fn remove(&mut self, id: &str) -> Result<Vec<ListRow>, String> {
            self.list_rows.retain(|f| f.0 != id);
            Ok(self.list())
        }
        fn generate(&mut self, size: usize, _: bool) -> Result<String, String> {
            Ok("G".repeat(size))
        }
        fn lock(&mut self) {
            self.locked += 1;
        }
        fn sync(&mut self) -> Result<Effect, String> {
            self.calls.push(("sync".into(), Vec::new()));
            self.response
                .take()
                .unwrap_or_else(|| Ok(Effect::List(self.list(), "Synced: ok.".into())))
        }
        fn dialog(&mut self, t: DialogKind, v: Vec<Zeroizing<String>>) -> Result<Effect, String> {
            self.calls
                .push((format!("{t:?}"), v.iter().map(|x| x.to_string()).collect()));
            self.response
                .take()
                .unwrap_or_else(|| Ok(Effect::Warning("Done.".into())))
        }
        fn pending_warning(&mut self) -> Option<String> {
            self.extra.take()
        }
    }

    fn ctrl(c: char) -> KeyEvent {
        let mut e = k(KeyCode::Char(c));
        e.modifiers = KeyModifiers::CONTROL;
        e
    }

    fn open() -> (App, Fake) {
        let mut sv = Fake::new();
        let mut a = App::locked(true);
        a.unlock_result(sv.unlock("x"), Instant::now());
        (a, sv)
    }

    #[test]
    fn adding_via_the_form_saves_and_selects_the_new_entry() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('a')), t0, &mut no_copy());
        assert!(screen(&a).contains("New account"));
        type_text(&mut a, "Zeta", t0);
        a.keypress(k(KeyCode::Tab), t0, &mut no_copy());
        type_text(&mut a, "zed", t0);
        a.keypress(k(KeyCode::Tab), t0, &mut no_copy());
        type_text(&mut a, "pw1", t0);
        let s = screen(&a);
        assert!(s.contains("•••") && !s.contains("pw1"), "password masked");
        a.keypress(ctrl('s'), t0, &mut no_copy());
        serve(&mut a, &mut sv, t0);
        let s = screen(&a);
        assert!(!s.contains("New account") && s.contains("Zeta") && s.contains("Account added"));
        // The new entry is the selected one: reveal shows its password.
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        assert!(screen(&a).contains("pw1"));
        assert_eq!(sv.list_rows.len(), 3);
    }

    #[test]
    fn title_is_required_and_nothing_is_requested() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('a')), t0, &mut no_copy());
        a.keypress(ctrl('s'), t0, &mut no_copy());
        assert!(a.request.is_none());
        assert!(screen(&a).contains("Title is required"));
        serve(&mut a, &mut sv, t0);
        assert_eq!(sv.list_rows.len(), 2);
    }

    #[test]
    fn esc_cancels_and_discards_what_was_typed() {
        let (mut a, _) = open();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('a')), t0, &mut no_copy());
        type_text(&mut a, "draft", t0);
        a.keypress(k(KeyCode::Esc), t0, &mut no_copy());
        assert!(a.request.is_none());
        let s = screen(&a);
        assert!(!s.contains("New account") && !s.contains("draft"));
    }

    #[test]
    fn generating_a_password_fills_the_field_and_shows_it() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('a')), t0, &mut no_copy());
        a.keypress(ctrl('g'), t0, &mut no_copy());
        assert!(matches!(
            a.request,
            Some(Request::Generate {
                size: 20,
                symbols: true
            })
        ));
        serve(&mut a, &mut sv, t0);
        assert!(screen(&a).contains(&"G".repeat(20)));
        a.keypress(ctrl('r'), t0, &mut no_copy());
        assert!(!screen(&a).contains(&"G".repeat(20)), "Ctrl+R hides de new");
    }

    #[test]
    fn generator_length_and_symbols_go_in_the_request() {
        let (mut a, _) = open();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('a')), t0, &mut no_copy());
        for _ in 0..3 {
            a.keypress(k(KeyCode::Tab), t0, &mut no_copy());
        }
        a.keypress(k(KeyCode::Left), t0, &mut no_copy());
        a.keypress(k(KeyCode::Char(' ')), t0, &mut no_copy());
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        assert!(matches!(
            a.request,
            Some(Request::Generate {
                size: 19,
                symbols: false
            })
        ));
    }

    #[test]
    fn edit_fills_the_form_and_keeps_the_selection() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Down), t0, &mut no_copy());
        a.keypress(k(KeyCode::Char('e')), t0, &mut no_copy());
        let s = screen(&a);
        assert!(s.contains("Edit account") && s.contains("Email"));
        type_text(&mut a, "2", t0);
        a.keypress(ctrl('s'), t0, &mut no_copy());
        serve(&mut a, &mut sv, t0);
        assert_eq!(sv.last.as_ref().unwrap().0.as_deref(), Some("id-email"));
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        let s = screen(&a);
        assert!(s.contains("Email2") && s.contains("secret-email"), "{s}");
        assert!(!s.contains("secret-bank"));
    }

    #[test]
    fn multiline_notes_come_back_identical() {
        let (mut a, mut sv) = open();
        sv.notes = "a\nb".into();
        a.unlock_result(sv.unlock("x"), Instant::now());
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('e')), t0, &mut no_copy());
        assert!(screen(&a).contains("a↵b"));
        a.keypress(ctrl('s'), t0, &mut no_copy());
        serve(&mut a, &mut sv, t0);
        assert_eq!(sv.last.unwrap().2, "a\nb");
    }

    #[test]
    fn service_error_stays_in_the_form_without_losing_what_was_typed() {
        let (mut a, mut sv) = open();
        sv.failure = Some("disk full [io]".into());
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('a')), t0, &mut no_copy());
        type_text(&mut a, "Zeta", t0);
        a.keypress(ctrl('s'), t0, &mut no_copy());
        serve(&mut a, &mut sv, t0);
        let s = screen(&a);
        assert!(s.contains("disk full") && s.contains("Zeta") && s.contains("New account"));
        // Tries again after the problem passes.
        sv.failure = None;
        a.keypress(ctrl('s'), t0, &mut no_copy());
        serve(&mut a, &mut sv, t0);
        assert!(!screen(&a).contains("New account"));
    }

    #[test]
    fn remove_asks_for_confirmation() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('d')), t0, &mut no_copy());
        assert!(screen(&a).contains("Remove \"Bank\"?"));
        a.keypress(k(KeyCode::Char('n')), t0, &mut no_copy());
        assert!(a.request.is_none() && !screen(&a).contains("Remove \""));
        // Enter also cancels: the default is not to remove.
        a.keypress(k(KeyCode::Char('d')), t0, &mut no_copy());
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        assert!(a.request.is_none());
        a.keypress(k(KeyCode::Char('d')), t0, &mut no_copy());
        a.keypress(k(KeyCode::Char('y')), t0, &mut no_copy());
        serve(&mut a, &mut sv, t0);
        let s = screen(&a);
        assert!(!s.contains("Bank") && s.contains("Email") && s.contains("Account removed"));
        assert_eq!(sv.list_rows.len(), 1);
    }

    #[test]
    fn remove_with_an_empty_list_does_not_break() {
        let mut a = App::new(Vec::new());
        let t0 = Instant::now();
        for c in ['d', 'e'] {
            a.keypress(k(KeyCode::Char(c)), t0, &mut no_copy());
        }
        assert!(!screen(&a).contains("Remove") && !screen(&a).contains("Edit account"));
    }

    #[test]
    fn mouse_on_the_form_focuses_field_and_triggers_buttons() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('a')), t0, &mut no_copy());
        let _ = screen(&a);
        // Click on the URL field and type there.
        let (x, y) = find_text(&a, "URL");
        a.mouse(
            m(MouseEventKind::Down(MouseButton::Left), x, y),
            t0,
            &mut no_copy(),
        );
        type_text(&mut a, "site", t0);
        assert!(screen(&a).contains("site"));
        // Save without a title: clicking the button shows the error.
        let (x, y) = find_text(&a, "Save (Ctrl+S)");
        a.mouse(
            m(MouseEventKind::Down(MouseButton::Left), x, y),
            t0,
            &mut no_copy(),
        );
        assert!(screen(&a).contains("Title is required"));
        // Generate button.
        let (x, y) = find_text(&a, "[Generate]");
        a.mouse(
            m(MouseEventKind::Down(MouseButton::Left), x, y),
            t0,
            &mut no_copy(),
        );
        serve(&mut a, &mut sv, t0);
        assert!(screen(&a).contains(&"G".repeat(20)));
        let (x, y) = find_text(&a, "Cancel (Esc)");
        a.mouse(
            m(MouseEventKind::Down(MouseButton::Left), x, y),
            t0,
            &mut no_copy(),
        );
        assert!(!screen(&a).contains("New account"));
    }

    #[test]
    fn mouse_on_the_confirmation_and_the_footer() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        let (x, y) = find_text(&a, "d del");
        a.mouse(
            m(MouseEventKind::Down(MouseButton::Left), x, y),
            t0,
            &mut no_copy(),
        );
        let _ = screen(&a);
        let (x, y) = find_text(&a, "Yes (y)");
        a.mouse(
            m(MouseEventKind::Down(MouseButton::Left), x, y),
            t0,
            &mut no_copy(),
        );
        serve(&mut a, &mut sv, t0);
        assert_eq!(sv.list_rows.len(), 1);
    }

    #[test]
    fn locking_closes_the_form_and_releases_the_vault() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('a')), t0, &mut no_copy());
        type_text(&mut a, "draft-secreto", t0);
        a.tick(t0 + Duration::from_secs(LOCK_SECONDS + 1), &mut |_| {});
        serve(&mut a, &mut sv, t0);
        let s = screen(&a);
        assert!(s.contains("Master password") && !s.contains("draft-secreto"));
        assert!(!s.contains("New account"));
        assert!(sv.locked >= 1, "the service released the vault");
    }

    #[test]
    fn footer_shows_the_new_actions_and_fits_in_80_columns() {
        let (a, _) = open();
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
        t.draw(|f| a.draw(f)).unwrap();
        let b = t.backend().buffer().clone();
        let last: String = (0..b.area.width).map(|x| b[(x, 22)].symbol()).collect();
        for p in ["add", "edit", "del", "lock", "quit"] {
            assert!(last.contains(p), "{last}");
        }
    }

    // --- sync, import/export, master password, first run -----

    #[test]
    fn key_s_syncs_and_shows_the_result() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('s')), t0, &mut no_copy());
        assert!(matches!(a.request, Some(Request::Sync)));
        assert!(screen(&a).contains("Syncing"));
        serve(&mut a, &mut sv, t0);
        assert!(screen(&a).contains("Synced: ok."));
        assert_eq!(sv.calls[0].0, "sync");
    }

    #[test]
    fn sync_failure_shows_as_a_warning_and_the_extra_warning_is_joined() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        sv.response = Some(Err("no sync folder yet: press f".into()));
        a.keypress(k(KeyCode::Char('s')), t0, &mut no_copy());
        serve(&mut a, &mut sv, t0);
        assert!(screen(&a).contains("no sync folder yet"));
        // The service warning after saving joins the result.
        sv.extra = Some("sync failed: offline".into());
        a.keypress(k(KeyCode::Char('a')), t0, &mut no_copy());
        type_text(&mut a, "Zeta", t0);
        a.keypress(ctrl('s'), t0, &mut no_copy());
        serve(&mut a, &mut sv, t0);
        let s = screen(&a);
        assert!(
            s.contains("Account added") && s.contains("sync failed: offline"),
            "{s}"
        );
    }

    #[test]
    fn import_takes_both_fields_and_closes_with_the_result() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        sv.response = Some(Ok(Effect::List(sv.list(), "Imported: 2.".into())));
        a.keypress(k(KeyCode::Char('i')), t0, &mut no_copy());
        assert!(screen(&a).contains("Import"));
        type_text(&mut a, " contas.csv ", t0);
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        serve(&mut a, &mut sv, t0);
        assert_eq!(
            sv.calls[0],
            (
                "Import".to_owned(),
                vec!["contas.csv".to_owned(), String::new()]
            )
        );
        let s = screen(&a);
        assert!(s.contains("Imported: 2.") && !s.contains("File (.csv/.kdbx)"));
    }

    #[test]
    fn dialog_without_a_path_does_not_call_the_service_and_esc_cancels() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('x')), t0, &mut no_copy());
        a.keypress(ctrl('s'), t0, &mut no_copy());
        assert!(screen(&a).contains("Enter the file path"));
        serve(&mut a, &mut sv, t0);
        assert!(sv.calls.is_empty());
        a.keypress(k(KeyCode::Esc), t0, &mut no_copy());
        assert!(!screen(&a).contains("File to create"));
    }

    #[test]
    fn service_error_stays_in_the_dialog() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        sv.response = Some(Err("contas.csv already exists".into()));
        a.keypress(k(KeyCode::Char('x')), t0, &mut no_copy());
        type_text(&mut a, "contas.csv", t0);
        a.keypress(ctrl('s'), t0, &mut no_copy());
        serve(&mut a, &mut sv, t0);
        let s = screen(&a);
        assert!(s.contains("already exists") && s.contains("File to create"));
    }

    #[test]
    fn change_password_checks_length_and_repetition() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('p')), t0, &mut no_copy());
        type_text(&mut a, "curta", t0);
        a.keypress(ctrl('s'), t0, &mut no_copy());
        assert!(screen(&a).contains("at least 8"));
        for _ in 0..5 {
            a.keypress(k(KeyCode::Backspace), t0, &mut no_copy());
        }
        type_text(&mut a, "password-new-123", t0);
        a.keypress(k(KeyCode::Tab), t0, &mut no_copy());
        type_text(&mut a, "password-new-124", t0);
        a.keypress(ctrl('s'), t0, &mut no_copy());
        let s = screen(&a);
        assert!(
            s.contains("do not match") && !s.contains("password-new"),
            "{s}"
        );
        serve(&mut a, &mut sv, t0);
        assert!(sv.calls.is_empty(), "nothing reached the service");
    }

    #[test]
    fn recovery_key_stays_until_dismissed_then_disappears() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        sv.response = Some(Ok(Effect::Key(
            Zeroizing::new("ABCD-EFGH-IJKL".into()),
            None,
        )));
        a.keypress(k(KeyCode::Char('p')), t0, &mut no_copy());
        type_text(&mut a, "password-new-123", t0);
        a.keypress(k(KeyCode::Tab), t0, &mut no_copy());
        type_text(&mut a, "password-new-123", t0);
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        serve(&mut a, &mut sv, t0);
        assert_eq!(sv.calls[0].1, ["password-new-123", "password-new-123"]);
        let s = screen(&a);
        assert!(s.contains("Recovery key") && s.contains("ABCD-EFGH-IJKL"));
        // Other keys do not dismiss it by mistake; Enter dismisses.
        a.keypress(k(KeyCode::Char('q')), t0, &mut no_copy());
        assert!(!a.quit && screen(&a).contains("ABCD-EFGH-IJKL"));
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        assert!(!screen(&a).contains("ABCD-EFGH-IJKL"));
    }

    #[test]
    fn mouse_on_the_dialog_and_the_message() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        sv.response = Some(Ok(Effect::Key(Zeroizing::new("KEY-XYZ".into()), None)));
        a.keypress(k(KeyCode::Char('p')), t0, &mut no_copy());
        type_text(&mut a, "password-new-123", t0);
        a.keypress(k(KeyCode::Tab), t0, &mut no_copy());
        type_text(&mut a, "password-new-123", t0);
        let _ = screen(&a);
        let (x, y) = find_text(&a, "OK (Ctrl+S)");
        a.mouse(
            m(MouseEventKind::Down(MouseButton::Left), x, y),
            t0,
            &mut no_copy(),
        );
        serve(&mut a, &mut sv, t0);
        assert!(screen(&a).contains("KEY-XYZ"));
        let (x, y) = find_text(&a, "OK (Enter)");
        a.mouse(
            m(MouseEventKind::Down(MouseButton::Left), x, y),
            t0,
            &mut no_copy(),
        );
        assert!(!screen(&a).contains("KEY-XYZ"));
    }

    #[test]
    fn first_run_creates_the_vault_shows_the_key_and_enters() {
        let mut sv = Fake::new();
        sv.list_rows.clear();
        sv.response = Some(Ok(Effect::Key(
            Zeroizing::new("NEW-KEY-1".into()),
            Some(Vec::new()),
        )));
        let mut a = App::locked(false);
        let t0 = Instant::now();
        assert!(screen(&a).contains("Press Enter to create"));
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        assert!(screen(&a).contains("Create your vault"));
        type_text(&mut a, "password-inicial-1", t0);
        a.keypress(k(KeyCode::Tab), t0, &mut no_copy());
        type_text(&mut a, "password-inicial-1", t0);
        a.keypress(ctrl('s'), t0, &mut no_copy());
        serve(&mut a, &mut sv, t0);
        assert_eq!(sv.calls[0].0, "Create");
        let s = screen(&a);
        assert!(s.contains("NEW-KEY-1"));
        a.keypress(k(KeyCode::Enter), t0, &mut no_copy());
        let s = screen(&a);
        assert!(
            !s.contains("Master password") && s.contains("no entries"),
            "{s}"
        );
        // Now the app works as unlocked.
        a.keypress(k(KeyCode::Char('a')), t0, &mut no_copy());
        assert!(screen(&a).contains("New account"));
    }

    #[test]
    fn locking_discards_open_dialog_and_message() {
        let (mut a, mut sv) = open();
        let t0 = Instant::now();
        a.keypress(k(KeyCode::Char('p')), t0, &mut no_copy());
        type_text(&mut a, "password-secreta-1", t0);
        a.tick(t0 + Duration::from_secs(LOCK_SECONDS + 1), &mut |_| {});
        serve(&mut a, &mut sv, t0);
        let s = screen(&a);
        assert!(s.contains("Master password") && !s.contains("Change master password"));
    }

    #[test]
    fn two_line_footer_lists_the_new_actions_in_80_columns() {
        let (a, _) = open();
        let mut t = Terminal::new(TestBackend::new(80, 24)).unwrap();
        t.draw(|f| a.draw(f)).unwrap();
        let b = t.backend().buffer().clone();
        let line = |y: u16| -> String { (0..b.area.width).map(|x| b[(x, y)].symbol()).collect() };
        let (l1, l2) = (line(22), line(23));
        for p in ["add", "edit", "del", "lock", "quit"] {
            assert!(l1.contains(p), "{l1}");
        }
        for p in ["sync", "import", "export", "password", "folder"] {
            assert!(l2.contains(p), "{l2}");
        }
    }
}
