//! Terminal look, in the Kotsu style ("Cotton Candy" theme).
//!
//! Colors and decorations (hearts, clouds, little faces) only appear when the stream
//! is a terminal and `NO_COLOR` is not set. In a pipe, redirection or
//! test the output is plain text, with no glyphs, so it stays easy to filter
//! with `grep` and scripts. The palette is Kotsu's (night by default; light with
//! `NEKA_THEME=light`). No secret goes through here beyond what would already be
//! printed: the style only wraps the text, never changes it.

use std::fmt::Display;
use std::io::Write;
use std::sync::OnceLock;

use anstream::ColorChoice;
use clap::builder::styling::{Effects, RgbColor, Style, Styles};

#[derive(Clone, Copy)]
struct Palette {
    highlight: RgbColor,
    secondary: RgbColor,
    warning: RgbColor,
    ok: RgbColor,
    error: RgbColor,
    neutral: RgbColor,
    text: RgbColor,
    background: RgbColor,
    panel: RgbColor,
}

/// Cotton Candy night (default).
const NIGHT: Palette = Palette {
    highlight: RgbColor(0xff, 0x7f, 0xb8),
    secondary: RgbColor(0x8f, 0xb3, 0xff),
    warning: RgbColor(0xff, 0xd2, 0x7a),
    ok: RgbColor(0x86, 0xef, 0xc0),
    error: RgbColor(0xff, 0x7a, 0x9e),
    neutral: RgbColor(0x9a, 0x7f, 0xa8),
    text: RgbColor(0xff, 0xe9, 0xf4),
    background: RgbColor(0x1d, 0x14, 0x24),
    panel: RgbColor(0x41, 0x27, 0x4f),
};

/// Cotton Candy light, for white-background terminals.
const LIGHT: Palette = Palette {
    highlight: RgbColor(0xe0, 0x50, 0x9a),
    secondary: RgbColor(0x4f, 0x74, 0xc7),
    warning: RgbColor(0xb8, 0x74, 0x00),
    ok: RgbColor(0x1f, 0x9d, 0x6b),
    error: RgbColor(0xd6, 0x33, 0x6c),
    neutral: RgbColor(0x8f, 0x6f, 0x8c),
    text: RgbColor(0x5a, 0x3a, 0x5e),
    background: RgbColor(0xff, 0xf5, 0xfa),
    panel: RgbColor(0xff, 0xd6, 0xe8),
};

const MARKER: &str = "♡";
const CLOUD: &str = "☁";
const SPARK: &str = "✧";
const STAR: &str = "☆";
const HAPPY: &str = "(◕‿◕)♡";
const SAD: &str = "(；ω；)";
const ZEROED: &str = "zzz…";

/// Which output is being written.
#[derive(Clone, Copy, PartialEq, Eq)]
pub enum Flow {
    Output,
    Error,
}

/// Role of the color in the palette.
#[derive(Clone, Copy)]
pub enum Tone {
    Highlight,
    Secondary,
    Warning,
    Ok,
    Error,
    Neutral,
    Text,
}

fn palette() -> &'static Palette {
    static P: OnceLock<Palette> = OnceLock::new();
    P.get_or_init(|| match std::env::var("NEKA_THEME") {
        Ok(v) if matches!(v.to_lowercase().as_str(), "light") => LIGHT,
        _ => NIGHT,
    })
}

/// Colors and decorations only on a terminal, respecting `NO_COLOR` and `TERM=dumb`.
pub fn decorated(f: Flow) -> bool {
    static OUTPUT: OnceLock<bool> = OnceLock::new();
    static ERROR: OnceLock<bool> = OnceLock::new();
    match f {
        Flow::Output => *OUTPUT
            .get_or_init(|| anstream::AutoStream::choice(&std::io::stdout()) != ColorChoice::Never),
        Flow::Error => *ERROR
            .get_or_init(|| anstream::AutoStream::choice(&std::io::stderr()) != ColorChoice::Never),
    }
}

fn color(t: Tone) -> RgbColor {
    let p = palette();
    match t {
        Tone::Highlight => p.highlight,
        Tone::Secondary => p.secondary,
        Tone::Warning => p.warning,
        Tone::Ok => p.ok,
        Tone::Error => p.error,
        Tone::Neutral => p.neutral,
        Tone::Text => p.text,
    }
}

fn style(t: Tone, bold: bool) -> Style {
    let s = Style::new().fg_color(Some(color(t).into()));
    if bold {
        s.effects(Effects::BOLD)
    } else {
        s
    }
}

/// Text in the requested color; without decoration, returns the text as it came.
pub fn tone(f: Flow, t: Tone, bold: bool, text: impl Display) -> String {
    if decorated(f) {
        let e = style(t, bold);
        format!("{e}{text}{e:#}")
    } else {
        text.to_string()
    }
}

/// Writes a (styled) line to the requested output.
pub fn line(f: Flow, text: impl Display) {
    match f {
        Flow::Output => anstream::println!("{text}"),
        Flow::Error => anstream::eprintln!("{text}"),
    }
}

/// Success message: `♡ text` (only the heart gets color).
pub fn success(f: Flow, text: impl Display) {
    if decorated(f) {
        line(f, format!("{} {text}", tone(f, Tone::Ok, true, MARKER)));
    } else {
        line(f, text);
    }
}

/// Discreet hint or note: `☆ text` in a neutral tone.
pub fn hint(f: Flow, text: impl Display) {
    if decorated(f) {
        line(f, tone(f, Tone::Neutral, false, format!("{STAR} {text}")));
    } else {
        line(f, text);
    }
}

/// Warning: `✧ warning: text` in gold.
pub fn warning(text: impl Display) {
    let f = Flow::Error;
    if decorated(f) {
        line(
            f,
            tone(f, Tone::Warning, false, format!("{SPARK} warning: {text}")),
        );
    } else {
        line(f, format!("warning: {text}"));
    }
}

/// Error: `(；ω；) error: text` in reddish pink.
pub fn failure(text: impl Display) {
    let f = Flow::Error;
    if decorated(f) {
        line(
            f,
            tone(f, Tone::Error, true, format!("{SAD} error: {text}")),
        );
    } else {
        line(f, format!("error: {text}"));
    }
}

/// Section title: `☁ text ☁` (terminal only).
pub fn title(f: Flow, text: impl Display) {
    if decorated(f) {
        line(
            f,
            tone(f, Tone::Highlight, true, format!("{CLOUD} {text} {CLOUD}")),
        );
    }
}

/// "Label:  value" line, with the label aligned to 10 columns.
pub fn field(label: &str, value: impl Display) {
    wide_field(label, 10, value);
}

/// Like `field`, with the label width of your choice.
pub fn wide_field(label: &str, width: usize, value: impl Display) {
    let r = format!("{label:<width$}");
    line(
        Flow::Output,
        format!("{} {value}", tone(Flow::Output, Tone::Secondary, true, r)),
    );
}

/// Short piece of supporting text (ids, addresses, dates).
pub fn erased(text: impl Display) -> String {
    tone(Flow::Output, Tone::Neutral, false, text)
}

/// An entry in the listing: `♡ id  title  (username)  address  [tags]`.
pub fn entry_line(id: &str, title: &str, username: &str, url: &str, tags: &[String]) {
    let s = Flow::Output;
    if !decorated(s) {
        let mut l = format!("{id}  {title}");
        if !username.is_empty() {
            l.push_str(&format!("  ({username})"));
        }
        if !url.is_empty() {
            l.push_str(&format!("  {url}"));
        }
        if !tags.is_empty() {
            l.push_str(&format!("  [{}]", tags.join(", ")));
        }
        line(s, l);
        return;
    }
    let mut l = format!(
        "{} {}  {}",
        tone(s, Tone::Highlight, false, MARKER),
        tone(s, Tone::Neutral, false, id),
        tone(s, Tone::Text, true, title)
    );
    if !username.is_empty() {
        l.push_str(&format!(
            "  {}",
            tone(s, Tone::Secondary, false, format!("({username})"))
        ));
    }
    if !url.is_empty() {
        l.push_str(&format!("  {}", tone(s, Tone::Neutral, false, url)));
    }
    if !tags.is_empty() {
        l.push_str(&format!(
            "  {}",
            tone(s, Tone::Warning, false, format!("[{}]", tags.join(", ")))
        ));
    }
    line(s, l);
}

/// Listing footer: `(◕‿◕)♡ 3 entries`, or `zzz… no entries` if empty.
pub fn footer_list(n: usize) {
    let f = Flow::Error;
    let how_many = match n {
        0 => "no entries".to_owned(),
        1 => "1 entry".to_owned(),
        n => format!("{n} entries"),
    };
    if !decorated(f) {
        line(f, format!("{}.", capitalizar(&how_many)));
    } else if n == 0 {
        line(
            f,
            tone(f, Tone::Neutral, false, format!("{ZEROED} {how_many}")),
        );
    } else {
        line(
            f,
            format!(
                "{} {}",
                tone(f, Tone::Warning, true, HAPPY),
                tone(f, Tone::Neutral, false, how_many)
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

/// Text that precedes a request on input (prompt), with the heart in color.
pub fn prompt_text(text: &str) -> String {
    let f = Flow::Error;
    if decorated(f) {
        format!(
            "{} {}",
            tone(f, Tone::Highlight, true, MARKER),
            tone(f, Tone::Text, false, text)
        )
    } else {
        text.to_owned()
    }
}

/// Writes a styled prompt to the error output, without a line break.
pub fn prompt(text: &str) {
    anstream::eprint!("{}", prompt_text(text));
    let _ = anstream::stderr().flush();
}

/// Recovery key block. The key stays alone on a line.
pub fn recovery_key(key: &str) {
    let s = Flow::Output;
    line(s, "");
    if decorated(s) {
        title(s, "RECOVERY KEY");
        line(s, "");
        line(s, format!("    {}", tone(s, Tone::Warning, true, key)));
        line(s, "");
        line(
            s,
            tone(
                s,
                Tone::Warning,
                false,
                format!("{SPARK} Shown only now. Write it on paper and keep it away from this computer."),
            ),
        );
    } else {
        line(
            s,
            "RECOVERY KEY (shown only now; write it on paper and keep it away from this computer):",
        );
        line(s, "");
        line(s, format!("    {key}"));
        line(s, "");
    }
    line(
        s,
        "Without it and the master password, nobody can recover the vault.",
    );
}

/// `--help` colors, in Kotsu's palette.
pub fn clap_styles() -> Styles {
    Styles::styled()
        .header(style(Tone::Highlight, true))
        .usage(style(Tone::Highlight, true))
        .literal(style(Tone::Secondary, true))
        .placeholder(style(Tone::Warning, false))
        .valid(style(Tone::Ok, false))
        .invalid(style(Tone::Warning, false))
        .error(style(Tone::Error, true))
}

/// A row of the entries table. The password is only filled in with `--reveal`.
pub struct TableRow {
    pub id: String,
    pub title: String,
    pub username: String,
    pub password_empty: bool,
    pub password: Option<String>,
    pub password_date: String,
    pub tags: Vec<String>,
}

fn cut(s: &str, max: usize) -> String {
    if s.chars().count() <= max {
        s.to_owned()
    } else {
        let mut t: String = s.chars().take(max.saturating_sub(1)).collect();
        t.push('…');
        t
    }
}

/// Entries table in the Kotsu style (terminal only): account, username,
/// hidden (or revealed) password and the date the password was set.
pub fn entry_table(lines: &[TableRow], reveal: bool) {
    let s = Flow::Output;
    let header = ["ID", "Account", "User", "Password", "Set on", "Tags"];
    let mut cell: Vec<[String; 6]> = Vec::new();
    for l in lines {
        let password = match (&l.password, l.password_empty) {
            (Some(p), _) if reveal => p.clone(),
            (_, true) => "(empty)".to_owned(),
            _ => "••••••••".to_owned(),
        };
        cell.push([
            l.id.clone(),
            cut(&l.title, 30),
            cut(&l.username, 28),
            password,
            l.password_date.clone(),
            cut(&l.tags.join(", "), 24),
        ]);
    }
    let mut width = header.map(|c| c.chars().count());
    for line in &cell {
        for (i, c) in line.iter().enumerate() {
            width[i] = width[i].max(c.chars().count());
        }
    }
    let border = |left: &str, middle: &str, dir: &str| {
        let parts: Vec<String> = width.iter().map(|w| "─".repeat(w + 2)).collect();
        tone(
            s,
            Tone::Neutral,
            false,
            format!("{left}{}{dir}", parts.join(middle)),
        )
    };
    let v = tone(s, Tone::Neutral, false, "│");
    let build = |values: Vec<String>| {
        let parts: Vec<String> = values.into_iter().map(|c| format!(" {c} ")).collect();
        format!("{v}{}{v}", parts.join(&v))
    };
    let pad = |c: &str, i: usize| format!("{c:<w$}", w = width[i]);

    line(s, border("╭", "┬", "╮"));
    line(
        s,
        build(
            header
                .iter()
                .enumerate()
                .map(|(i, c)| tone(s, Tone::Secondary, true, pad(c, i)))
                .collect(),
        ),
    );
    line(s, border("├", "┼", "┤"));
    for l in &cell {
        let styles = [
            (Tone::Neutral, false),
            (Tone::Text, true),
            (Tone::Secondary, false),
            (Tone::Warning, reveal),
            (Tone::Neutral, false),
            (Tone::Warning, false),
        ];
        line(
            s,
            build(
                l.iter()
                    .enumerate()
                    .map(|(i, c)| tone(s, styles[i].0, styles[i].1, pad(c, i)))
                    .collect(),
            ),
        );
    }
    line(s, border("╰", "┴", "╯"));
}

/// Palette color as an RGB triple, for screens that draw on their own.
pub fn rgb(t: Tone) -> (u8, u8, u8) {
    let RgbColor(r, g, b) = color(t);
    (r, g, b)
}

/// Background and panel (selected row) of the palette, as RGB triples.
pub fn backgrounds() -> ((u8, u8, u8), (u8, u8, u8)) {
    let p = palette();
    let RgbColor(a, b, c) = p.background;
    let RgbColor(d, e, f) = p.panel;
    ((a, b, c), (d, e, f))
}
