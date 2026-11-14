//! Add/edit form and confirmation window of the interactive app.
//! State and drawing only: save, remove and generate password are requests the main
//! loop hands to the service (`tui::Service`).

use std::cell::RefCell;

use neka_core::api::{PASSWORD_LEN_MAX, PASSWORD_LEN_MIN};
use ratatui::crossterm::event::{KeyCode, KeyEvent, KeyModifiers};
use ratatui::layout::{Alignment, Position, Rect};
use ratatui::style::Modifier;
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, Clear, Paragraph};
use ratatui::Frame;
use zeroize::{Zeroize, Zeroizing};

use crate::tui::style;
use crate::ui::Tone;

pub const DEFAULT_LEN: usize = 20;

/// Editable text with a cursor (position in characters). Zeroed on drop.
#[derive(Default)]
pub struct Text {
    s: String,
    pos: usize,
}

impl Drop for Text {
    fn drop(&mut self) {
        self.s.zeroize();
    }
}

impl Text {
    pub fn de(s: &str) -> Self {
        Text {
            s: s.to_owned(),
            pos: s.chars().count(),
        }
    }

    pub fn text(&self) -> &str {
        &self.s
    }

    fn byte(&self, chars: usize) -> usize {
        self.s
            .char_indices()
            .nth(chars)
            .map_or(self.s.len(), |(i, _)| i)
    }

    fn size(&self) -> usize {
        self.s.chars().count()
    }

    pub fn insert(&mut self, c: char) {
        let i = self.byte(self.pos);
        self.s.insert(i, c);
        self.pos += 1;
    }

    fn delete_back(&mut self) {
        if self.pos > 0 {
            let i = self.byte(self.pos - 1);
            self.s.remove(i);
            self.pos -= 1;
        }
    }

    fn delete_forward(&mut self) {
        if self.pos < self.size() {
            let i = self.byte(self.pos);
            self.s.remove(i);
        }
    }

    fn edit(&mut self, k: KeyCode) {
        match k {
            KeyCode::Backspace => self.delete_back(),
            KeyCode::Delete => self.delete_forward(),
            KeyCode::Left => self.pos = self.pos.saturating_sub(1),
            KeyCode::Right => self.pos = (self.pos + 1).min(self.size()),
            KeyCode::Home => self.pos = 0,
            KeyCode::End => self.pos = self.size(),
            _ => {}
        }
    }

    pub fn replace(&mut self, new: &str) {
        self.s.zeroize();
        self.s = new.to_owned();
        self.pos = self.size();
    }
}

/// What the form hands over to save. Password and other fields are zeroed
/// on drop.
pub struct Data {
    pub title: Zeroizing<String>,
    pub username: Zeroizing<String>,
    pub password: Zeroizing<String>,
    pub url: Zeroizing<String>,
    pub notes: Zeroizing<String>,
    pub tags: Vec<String>,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Field {
    Title,
    Username,
    Password,
    Generator,
    Url,
    Tags,
    Notes,
}

const ORDER: [Field; 7] = [
    Field::Title,
    Field::Username,
    Field::Password,
    Field::Generator,
    Field::Url,
    Field::Tags,
    Field::Notes,
];

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Action {
    Nothing,
    Save,
    Cancel,
    Generate,
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Target {
    Field(usize),
    Generate,
    ViewPassword,
    Symbols,
    Save,
    Cancel,
}

pub struct Form {
    /// `Some(id)` when editing an existing entry.
    pub id: Option<String>,
    title: Text,
    username: Text,
    password: Text,
    url: Text,
    tags: Text,
    notes: Text,
    focus: usize,
    pub size: usize,
    pub symbols: bool,
    view_password: bool,
    pub error: Option<String>,
    pub busy: bool,
    targets: RefCell<Vec<(Rect, Target)>>,
}

pub struct Padding<'a> {
    pub id: &'a str,
    pub title: &'a str,
    pub username: &'a str,
    pub password: &'a str,
    pub url: &'a str,
    pub notes: &'a str,
    pub tags: &'a [String],
}

impl Form {
    pub fn new() -> Self {
        Form {
            id: None,
            title: Text::default(),
            username: Text::default(),
            password: Text::default(),
            url: Text::default(),
            tags: Text::default(),
            notes: Text::default(),
            focus: 0,
            size: DEFAULT_LEN,
            symbols: true,
            view_password: false,
            error: None,
            busy: false,
            targets: RefCell::new(Vec::new()),
        }
    }

    pub fn editing(p: Padding<'_>) -> Self {
        let mut f = Form::new();
        f.id = Some(p.id.to_owned());
        f.title = Text::de(p.title);
        f.username = Text::de(p.username);
        f.password = Text::de(p.password);
        f.url = Text::de(p.url);
        f.tags = Text::de(&p.tags.join(", "));
        f.notes = Text::de(p.notes);
        f
    }

    fn field(&mut self, c: Field) -> Option<&mut Text> {
        match c {
            Field::Title => Some(&mut self.title),
            Field::Username => Some(&mut self.username),
            Field::Password => Some(&mut self.password),
            Field::Url => Some(&mut self.url),
            Field::Tags => Some(&mut self.tags),
            Field::Notes => Some(&mut self.notes),
            Field::Generator => None,
        }
    }

    pub fn focus(&self) -> Field {
        ORDER[self.focus]
    }

    fn shift(&mut self, delta: isize) {
        let n = ORDER.len() as isize;
        self.focus = (self.focus as isize + delta).rem_euclid(n) as usize;
    }

    pub fn title_empty(&self) -> bool {
        self.title.text().trim().is_empty()
    }

    pub fn generated_password(&mut self, new: &str) {
        self.password.replace(new);
        self.view_password = true;
        self.error = None;
        self.busy = false;
    }

    pub fn data(&self) -> Data {
        let z = |t: &Text| Zeroizing::new(t.text().to_owned());
        Data {
            title: Zeroizing::new(self.title.text().trim().to_owned()),
            username: z(&self.username),
            password: z(&self.password),
            url: z(&self.url),
            notes: z(&self.notes),
            tags: self
                .tags
                .text()
                .split(',')
                .map(str::trim)
                .filter(|t| !t.is_empty())
                .map(str::to_owned)
                .collect(),
        }
    }

    fn resize(&mut self, delta: isize) {
        self.size = (self.size as isize + delta)
            .clamp(PASSWORD_LEN_MIN as isize, PASSWORD_LEN_MAX as isize)
            as usize;
    }

    pub fn keypress(&mut self, k: KeyEvent) -> Action {
        if self.busy {
            return Action::Nothing;
        }
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Esc => return Action::Cancel,
            KeyCode::Char('s') if ctrl => return Action::Save,
            KeyCode::Char('g') if ctrl => return Action::Generate,
            KeyCode::Char('r') if ctrl => {
                self.view_password = !self.view_password;
                return Action::Nothing;
            }
            KeyCode::Tab | KeyCode::Down => {
                self.shift(1);
                return Action::Nothing;
            }
            KeyCode::BackTab | KeyCode::Up => {
                self.shift(-1);
                return Action::Nothing;
            }
            KeyCode::Enter => {
                return match self.focus() {
                    Field::Generator => Action::Generate,
                    Field::Notes => Action::Save,
                    _ => {
                        self.shift(1);
                        Action::Nothing
                    }
                };
            }
            _ => {}
        }
        let focus = self.focus();
        if focus == Field::Generator {
            match k.code {
                KeyCode::Left => self.resize(-1),
                KeyCode::Right => self.resize(1),
                KeyCode::Char(' ') | KeyCode::Char('x') => self.symbols = !self.symbols,
                _ => {}
            }
            return Action::Nothing;
        }
        if ctrl {
            if k.code == KeyCode::Char('j') && focus == Field::Notes {
                self.notes.insert('\n');
            }
            return Action::Nothing;
        }
        if let Some(t) = self.field(focus) {
            match k.code {
                KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::ALT) => t.insert(c),
                other => t.edit(other),
            }
        }
        Action::Nothing
    }

    /// Click: focuses a field or triggers a button.
    pub fn click(&mut self, x: u16, y: u16) -> Action {
        if self.busy {
            return Action::Nothing;
        }
        let pos = Position::new(x, y);
        let target = self
            .targets
            .borrow()
            .iter()
            .rev() // the button, drawn after the row, wins over the row
            .find(|(r, _)| r.contains(pos))
            .map(|(_, a)| *a);
        match target {
            Some(Target::Field(i)) => {
                self.focus = i;
                Action::Nothing
            }
            Some(Target::Generate) => Action::Generate,
            Some(Target::ViewPassword) => {
                self.view_password = !self.view_password;
                Action::Nothing
            }
            Some(Target::Symbols) => {
                self.symbols = !self.symbols;
                Action::Nothing
            }
            Some(Target::Save) => Action::Save,
            Some(Target::Cancel) => Action::Cancel,
            None => Action::Nothing,
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let width = area.width.min(70);
        let height = area.height.min(15);
        let popup = Rect {
            x: area.x + (area.width - width) / 2,
            y: area.y + (area.height - height) / 2,
            width,
            height,
        };
        f.render_widget(Clear, popup);
        let title = if self.id.is_some() {
            " Edit account "
        } else {
            " New account "
        };
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(style(Tone::Highlight))
            .title(Span::styled(
                title,
                style(Tone::Highlight).add_modifier(Modifier::BOLD),
            ))
            .style(style(Tone::Text));
        let internal = block.inner(popup);
        f.render_widget(block, popup);

        let mut targets = Vec::new();
        let label = |t: &str, active: bool| {
            Span::styled(
                format!("{t:<10}"),
                style(if active {
                    Tone::Highlight
                } else {
                    Tone::Secondary
                })
                .add_modifier(Modifier::BOLD),
            )
        };
        let field_width = internal.width.saturating_sub(10) as usize;
        let mut cursor: Option<(u16, u16)> = None;

        for (i, c) in ORDER.iter().enumerate() {
            let y = internal.y + i as u16;
            if y >= internal.y + internal.height {
                break;
            }
            let active = i == self.focus;
            let line_rect = Rect::new(internal.x, y, internal.width, 1);
            targets.push((line_rect, Target::Field(i)));
            let name = match c {
                Field::Title => "Title",
                Field::Username => "User",
                Field::Password => "Password",
                Field::Generator => "Generate",
                Field::Url => "URL",
                Field::Tags => "Tags",
                Field::Notes => "Notes",
            };
            let mut spans = vec![label(name, active)];
            if *c == Field::Generator {
                let mark = if self.symbols { "x" } else { " " };
                let txt = format!("length {} (←/→)  [{mark}] symbols  [Generate]", self.size);
                // Clickable buttons inside the row: symbols and generate.
                let start_sim = internal.x
                    + 10
                    + format!("length {} (←/→)  ", self.size).chars().count() as u16;
                targets.push((Rect::new(start_sim, y, 11, 1), Target::Symbols));
                targets.push((Rect::new(start_sim + 13, y, 10, 1), Target::Generate));
                spans.push(Span::styled(txt, style(Tone::Text)));
            } else {
                let t = match c {
                    Field::Title => &self.title,
                    Field::Username => &self.username,
                    Field::Password => &self.password,
                    Field::Url => &self.url,
                    Field::Tags => &self.tags,
                    _ => &self.notes,
                };
                let mask = *c == Field::Password && !self.view_password;
                let (visible, cx) = window(t, field_width, mask);
                spans.push(Span::styled(
                    visible,
                    style(if active { Tone::Text } else { Tone::Secondary }),
                ));
                if active {
                    cursor = Some((internal.x + 10 + cx as u16, y));
                }
            }
            f.render_widget(Paragraph::new(Line::from(spans)), line_rect);
        }

        let y_msg = internal.y + ORDER.len() as u16 + 1;
        if y_msg < internal.y + internal.height {
            let msg = if self.busy {
                Span::styled("Saving…", style(Tone::Warning))
            } else if let Some(e) = &self.error {
                Span::styled(format!("✧ {e}"), style(Tone::Warning))
            } else {
                Span::styled(
                    "Tab/↑↓ move · Ctrl+G generate · Ctrl+R show/hide · Ctrl+J newline in notes",
                    style(Tone::Neutral),
                )
            };
            f.render_widget(
                Paragraph::new(Line::from(msg)),
                Rect::new(internal.x, y_msg, internal.width, 1),
            );
        }
        let y_bot = internal.y + internal.height.saturating_sub(1);
        if y_bot > y_msg {
            let buttons = " [ Save (Ctrl+S) ]   [ Cancel (Esc) ]   [ Show/hide password ] ";
            let start_s = internal.x + 1;
            targets.push((Rect::new(start_s, y_bot, 17, 1), Target::Save));
            targets.push((Rect::new(start_s + 20, y_bot, 16, 1), Target::Cancel));
            targets.push((Rect::new(start_s + 39, y_bot, 22, 1), Target::ViewPassword));
            f.render_widget(
                Paragraph::new(Line::from(Span::styled(
                    buttons,
                    style(Tone::Secondary).add_modifier(Modifier::BOLD),
                )))
                .alignment(Alignment::Left),
                Rect::new(internal.x, y_bot, internal.width, 1),
            );
        }
        *self.targets.borrow_mut() = targets;
        if let (Some((x, y)), false) = (cursor, self.busy) {
            f.set_cursor_position(Position::new(x, y));
        }
    }
}

/// Visible part of the text in a fixed width, with the cursor inside it.
fn window(t: &Text, width: usize, mask: bool) -> (String, usize) {
    let width = width.max(2);
    let chars: Vec<char> = t
        .text()
        .chars()
        .map(|c| {
            if mask {
                '•'
            } else if c == '\n' {
                '↵'
            } else {
                c
            }
        })
        .collect();
    let start = (t.pos + 1).saturating_sub(width);
    let end = (start + width).min(chars.len());
    (chars[start..end].iter().collect(), t.pos - start)
}

/// Yes/no question before removing.
pub struct Confirmation {
    pub id: String,
    pub title: String,
    pub busy: bool,
    targets: RefCell<Vec<(Rect, bool)>>,
}

impl Confirmation {
    pub fn new(id: String, title: String) -> Self {
        Confirmation {
            id,
            title,
            busy: false,
            targets: RefCell::new(Vec::new()),
        }
    }

    /// `Some(true)` confirms, `Some(false)` cancels.
    pub fn keypress(&self, k: KeyEvent) -> Option<bool> {
        if self.busy {
            return None;
        }
        match k.code {
            KeyCode::Char('y') | KeyCode::Char('Y') => Some(true),
            KeyCode::Char('n') | KeyCode::Char('N') | KeyCode::Esc | KeyCode::Enter => Some(false),
            _ => None,
        }
    }

    pub fn click(&self, x: u16, y: u16) -> Option<bool> {
        if self.busy {
            return None;
        }
        let pos = Position::new(x, y);
        self.targets
            .borrow()
            .iter()
            .find(|(r, _)| r.contains(pos))
            .map(|(_, sim)| *sim)
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let width = area.width.min(48);
        let height = area.height.min(7);
        let popup = Rect {
            x: area.x + (area.width - width) / 2,
            y: area.y + (area.height - height) / 2,
            width,
            height,
        };
        f.render_widget(Clear, popup);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(style(Tone::Warning))
            .title(Span::styled(
                " Remove account ",
                style(Tone::Warning).add_modifier(Modifier::BOLD),
            ))
            .style(style(Tone::Text));
        let internal = block.inner(popup);
        f.render_widget(block, popup);
        let name: String = self.title.chars().take(width as usize - 8).collect();
        let lines = vec![
            Line::from(Span::styled(
                format!("Remove \"{name}\"?"),
                style(Tone::Text).add_modifier(Modifier::BOLD),
            )),
            Line::from(Span::styled(
                if self.busy {
                    "Removing…"
                } else {
                    "It is deleted on every synced device."
                },
                style(Tone::Neutral),
            )),
            Line::from(""),
            Line::from(Span::styled(
                "[ Yes (y) ]   [ No (n/Esc) ]",
                style(Tone::Secondary).add_modifier(Modifier::BOLD),
            )),
        ];
        f.render_widget(Paragraph::new(lines).alignment(Alignment::Center), internal);
        let y = internal.y + 3;
        let button_width = 28u16;
        let x0 = internal.x + internal.width.saturating_sub(button_width) / 2;
        *self.targets.borrow_mut() = vec![
            (Rect::new(x0, y, 12, 1), true),
            (Rect::new(x0 + 14, y, 14, 1), false),
        ];
    }
}

// --- Simple field dialogs (import, export, sync folder, password) ---

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum DialogKind {
    Import,
    Export,
    SyncFolder,
    MasterPassword,
    Create,
}

struct DialogField {
    label: &'static str,
    text: Text,
    masked: bool,
}

pub struct Dialog {
    pub kind: DialogKind,
    title: &'static str,
    hint: &'static str,
    fields: Vec<DialogField>,
    focus: usize,
    pub error: Option<String>,
    pub busy: bool,
    targets: RefCell<Vec<(Rect, Target)>>,
}

impl Dialog {
    pub fn new(kind: DialogKind) -> Self {
        let field = |label, masked| DialogField {
            label,
            text: Text::default(),
            masked,
        };
        let (title, hint, fields) = match kind {
            DialogKind::Import => (
                " Import ",
                "CSV (Bitwarden, Chrome, KeePassXC, 1Password...) or KDBX.",
                vec![
                    field("File (.csv/.kdbx)", false),
                    field("KeePass password", true),
                ],
            ),
            DialogKind::Export => (
                " Export ",
                "KDBX is encrypted. A CSV is plain text on disk.",
                vec![
                    field("File to create", false),
                    field("KeePass password", true),
                    field("Type PLAINTEXT (csv)", false),
                ],
            ),
            DialogKind::SyncFolder => (
                " Sync folder ",
                "For example a folder inside Google Drive. Empty turns syncing off.",
                vec![field("Folder", false)],
            ),
            DialogKind::MasterPassword => (
                " Change master password ",
                "At least 8 characters (12+ is better). A new recovery key follows.",
                vec![field("New password", true), field("Repeat", true)],
            ),
            DialogKind::Create => (
                " Create your vault ",
                "Choose a master password (8+ characters, 12+ is better).",
                vec![field("Master password", true), field("Repeat", true)],
            ),
        };
        Dialog {
            kind,
            title,
            hint,
            fields,
            focus: 0,
            error: None,
            busy: false,
            targets: RefCell::new(Vec::new()),
        }
    }

    /// Typed values, in the order of the fields.
    pub fn values(&self) -> Vec<Zeroizing<String>> {
        self.fields
            .iter()
            .map(|c| Zeroizing::new(c.text.text().trim().to_owned()))
            .collect()
    }

    /// Checks what can be checked without touching the vault.
    pub fn validate(&self) -> Result<(), String> {
        let v = self.values();
        match self.kind {
            DialogKind::MasterPassword | DialogKind::Create => {
                let a = self.fields[0].text.text();
                let b = self.fields[1].text.text();
                if a.chars().count() < 8 {
                    return Err("The password must have at least 8 characters.".into());
                }
                if a != b {
                    return Err("The passwords do not match.".into());
                }
                Ok(())
            }
            DialogKind::Import | DialogKind::Export if v[0].is_empty() => {
                Err("Enter the file path.".into())
            }
            _ => Ok(()),
        }
    }

    pub fn keypress(&mut self, k: KeyEvent) -> Action {
        if self.busy {
            return Action::Nothing;
        }
        let n = self.fields.len();
        let ctrl = k.modifiers.contains(KeyModifiers::CONTROL);
        match k.code {
            KeyCode::Esc => return Action::Cancel,
            KeyCode::Char('s') if ctrl => return Action::Save,
            KeyCode::Tab | KeyCode::Down => self.focus = (self.focus + 1) % n,
            KeyCode::BackTab | KeyCode::Up => self.focus = (self.focus + n - 1) % n,
            KeyCode::Enter => {
                if self.focus + 1 == n {
                    return Action::Save;
                }
                self.focus += 1;
            }
            code if !ctrl => {
                let t = &mut self.fields[self.focus].text;
                match code {
                    KeyCode::Char(c) if !k.modifiers.contains(KeyModifiers::ALT) => t.insert(c),
                    other => t.edit(other),
                }
            }
            _ => {}
        }
        Action::Nothing
    }

    pub fn click(&mut self, x: u16, y: u16) -> Action {
        if self.busy {
            return Action::Nothing;
        }
        let pos = Position::new(x, y);
        let target = self
            .targets
            .borrow()
            .iter()
            .rev()
            .find(|(r, _)| r.contains(pos))
            .map(|(_, a)| *a);
        match target {
            Some(Target::Field(i)) => {
                self.focus = i;
                Action::Nothing
            }
            Some(Target::Save) => Action::Save,
            Some(Target::Cancel) => Action::Cancel,
            _ => Action::Nothing,
        }
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let width = area.width.min(72);
        let height = (self.fields.len() as u16 + 7).min(area.height);
        let popup = Rect {
            x: area.x + (area.width - width) / 2,
            y: area.y + (area.height - height) / 2,
            width,
            height,
        };
        f.render_widget(Clear, popup);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(style(Tone::Highlight))
            .title(Span::styled(
                self.title,
                style(Tone::Highlight).add_modifier(Modifier::BOLD),
            ))
            .style(style(Tone::Text));
        let internal = block.inner(popup);
        f.render_widget(block, popup);
        let max = internal.y + internal.height;
        let mut targets = Vec::new();
        let mut cursor = None;
        f.render_widget(
            Paragraph::new(Span::styled(self.hint, style(Tone::Neutral))),
            Rect::new(internal.x, internal.y, internal.width, 1),
        );
        let rot = 22u16;
        let field_width = internal.width.saturating_sub(rot) as usize;
        for (i, c) in self.fields.iter().enumerate() {
            let y = internal.y + 2 + i as u16;
            if y >= max {
                break;
            }
            let active = i == self.focus;
            let line = Rect::new(internal.x, y, internal.width, 1);
            targets.push((line, Target::Field(i)));
            let (visible, cx) = window(&c.text, field_width, c.masked);
            f.render_widget(
                Paragraph::new(Line::from(vec![
                    Span::styled(
                        format!("{:<w$}", c.label, w = rot as usize),
                        style(if active {
                            Tone::Highlight
                        } else {
                            Tone::Secondary
                        })
                        .add_modifier(Modifier::BOLD),
                    ),
                    Span::styled(
                        visible,
                        style(if active { Tone::Text } else { Tone::Secondary }),
                    ),
                ])),
                line,
            );
            if active {
                cursor = Some((internal.x + rot + cx as u16, y));
            }
        }
        let y_msg = internal.y + 3 + self.fields.len() as u16;
        if y_msg < max {
            let msg = if self.busy {
                Span::styled("Working…", style(Tone::Warning))
            } else if let Some(e) = &self.error {
                Span::styled(format!("✧ {e}"), style(Tone::Warning))
            } else {
                Span::styled("Tab/↑↓ move · Enter next/confirm", style(Tone::Neutral))
            };
            f.render_widget(
                Paragraph::new(msg),
                Rect::new(internal.x, y_msg, internal.width, 1),
            );
        }
        let y_bot = max.saturating_sub(1);
        if y_bot > y_msg {
            targets.push((Rect::new(internal.x + 1, y_bot, 17, 1), Target::Save));
            targets.push((Rect::new(internal.x + 20, y_bot, 16, 1), Target::Cancel));
            f.render_widget(
                Paragraph::new(Span::styled(
                    " [ OK (Ctrl+S) ]   [ Cancel (Esc) ]",
                    style(Tone::Secondary).add_modifier(Modifier::BOLD),
                )),
                Rect::new(internal.x, y_bot, internal.width, 1),
            );
        }
        *self.targets.borrow_mut() = targets;
        if let (Some((x, y)), false) = (cursor, self.busy) {
            f.set_cursor_position(Position::new(x, y));
        }
    }
}

/// Notice window that stays until dismissed (for example, the recovery
/// key, which only appears once). The text is zeroed on drop.
pub struct Message {
    title: &'static str,
    text: Zeroizing<String>,
    footer: &'static str,
    target: RefCell<Option<Rect>>,
}

impl Message {
    pub fn recovery_key(text: Zeroizing<String>) -> Self {
        Message {
            title: " Recovery key ",
            text,
            footer: "Write it down and keep it safe. It will not be shown again. [ OK (Enter) ]",
            target: RefCell::new(None),
        }
    }

    pub fn dismiss(&self, k: KeyEvent) -> bool {
        matches!(k.code, KeyCode::Enter | KeyCode::Esc)
    }

    pub fn click(&self, x: u16, y: u16) -> bool {
        self.target
            .borrow()
            .is_some_and(|r| r.contains(Position::new(x, y)))
    }

    pub fn draw(&self, f: &mut Frame, area: Rect) {
        let width = area.width.min(80);
        let height = area.height.min(9);
        let popup = Rect {
            x: area.x + (area.width - width) / 2,
            y: area.y + (area.height - height) / 2,
            width,
            height,
        };
        f.render_widget(Clear, popup);
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(style(Tone::Warning))
            .title(Span::styled(
                self.title,
                style(Tone::Warning).add_modifier(Modifier::BOLD),
            ))
            .style(style(Tone::Text));
        let internal = block.inner(popup);
        f.render_widget(block, popup);
        let lines = vec![
            Line::from(""),
            Line::from(Span::styled(
                self.text.as_str().to_owned(),
                style(Tone::Warning).add_modifier(Modifier::BOLD),
            )),
            Line::from(""),
            Line::from(Span::styled(self.footer, style(Tone::Neutral))),
        ];
        f.render_widget(
            Paragraph::new(lines)
                .alignment(Alignment::Center)
                .wrap(ratatui::widgets::Wrap { trim: true }),
            internal,
        );
        // The footer (with the OK button) takes the bottom lines of the window.
        *self.target.borrow_mut() = Some(Rect::new(
            internal.x,
            internal.y + 3,
            internal.width,
            internal.height.saturating_sub(3),
        ));
    }
}
