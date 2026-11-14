//! `neka`: command-line interface. Only arguments, prompts, clipboard
//! and formatting; the rules live in `neka_core::api`.

mod config;
mod forms;
mod service;
mod terminal;
mod tui;
mod ui;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use neka_core::api::{
    generate_csv, generate_kdbx, generate_password, read_csv, read_kdbx, recovery_key_to_text,
    ApiError, Classes, Credential, Entry, KdfParams, NewEntry, SyncReport, Uuid, Vault, VERSION,
};
use zeroize::Zeroizing;

use config::Config;
use ui::{Flow, Tone};

/// A cozy password manager for your terminal. The vault is a single encrypted
/// file and can sync through a Google Drive folder.
#[derive(Parser)]
#[command(name = "neka", version = VERSION, about, styles = ui::clap_styles())]
struct Cli {
    /// Data folder (default: %APPDATA%\neka on Windows).
    #[arg(long, global = true, value_name = "FOLDER")]
    dir: Option<PathBuf>,
    /// Do not sync automatically in this command.
    #[arg(long = "no-sync", global = true)]
    no_sync: bool,
    /// Without a command, neka opens the interactive screen.
    #[command(subcommand)]
    command: Option<Command>,
}

#[derive(Subcommand)]
enum Command {
    /// Create a new vault on this device.
    #[command(name = "create", aliases = ["init"])]
    Create,
    /// Bring a vault that is already in the cloud folder to this device.
    #[command(name = "clone")]
    Clone {
        /// .neka file in the synced folder.
        file: PathBuf,
    },
    /// Add an entry. The password is asked without echo, or generated with --generate.
    #[command(name = "add")]
    Add {
        /// Entry title.
        title: String,
        /// Username or e-mail.
        #[arg(short = 'u', long = "user")]
        username: Option<String>,
        /// Address (URL) of the site or service.
        #[arg(long)]
        url: Option<String>,
        /// Free-form notes.
        #[arg(long = "notes")]
        notes: Option<String>,
        /// Tag (can be repeated).
        #[arg(short = 't', long = "tag")]
        tags: Vec<String>,
        /// Generate a password with N characters (default 20).
        #[arg(short = 'g', long = "generate", value_name = "N", num_args = 0..=1, default_missing_value = "20")]
        generate: Option<usize>,
        /// Generate without symbols (for sites that reject them).
        #[arg(long = "no-symbols")]
        no_symbols: bool,
    },
    /// List entries, optionally filtered by text or tag.
    #[command(name = "list", aliases = ["ls"])]
    List {
        /// Text to look for in title, username or address.
        search: Option<String>,
        /// Only entries with this tag.
        #[arg(short = 't', long = "tag")]
        tag: Option<String>,
        /// Unlock the view: show the passwords in the list (they stay in the
        /// terminal scrollback).
        #[arg(short = 'r', long = "reveal")]
        reveal: bool,
    },
    /// Show an entry (the password only with --password).
    #[command(name = "show")]
    Show {
        /// Entry id (4+ characters) or title.
        entry: String,
        /// Reveal the password.
        #[arg(short = 'p', long = "password", short_alias = 's')]
        password: bool,
        /// Also show old passwords.
        #[arg(long = "history")]
        history: bool,
    },
    /// Copy the password (or the username) to the clipboard.
    #[command(name = "copy", aliases = ["cp"])]
    Copy {
        /// Entry id (4+ characters) or title.
        entry: String,
        /// Copy the username instead of the password.
        #[arg(short = 'u', long = "user")]
        username: bool,
        /// Seconds until the clipboard is cleared (0 = never clear).
        #[arg(long = "seconds", default_value_t = 20)]
        seconds: u64,
    },
    /// Edit an entry. --password asks for a new password; --generate makes one.
    #[command(name = "edit")]
    Edit {
        /// Entry id (4+ characters) or title.
        entry: String,
        /// New title.
        #[arg(long = "title")]
        title: Option<String>,
        /// New username.
        #[arg(short = 'u', long = "user")]
        username: Option<String>,
        /// New address (URL).
        #[arg(long)]
        url: Option<String>,
        /// New notes.
        #[arg(long = "notes")]
        notes: Option<String>,
        /// Replace the tags (can be repeated).
        #[arg(short = 't', long = "tag")]
        tags: Vec<String>,
        /// Remove all tags.
        #[arg(long = "no-tags")]
        no_tags: bool,
        /// Ask for a new password.
        #[arg(short = 'p', long = "password", short_alias = 's')]
        password: bool,
        /// Generate a new password with N characters (default 20).
        #[arg(short = 'g', long = "generate", value_name = "N", num_args = 0..=1, default_missing_value = "20")]
        generate: Option<usize>,
        /// Generate without symbols.
        #[arg(long = "no-symbols")]
        no_symbols: bool,
    },
    /// Remove an entry.
    #[command(name = "remove", aliases = ["rm"])]
    Remove {
        /// Entry id (4+ characters) or title.
        entry: String,
        /// Do not ask for confirmation.
        #[arg(short = 'y', long = "yes", short_alias = 's')]
        sim: bool,
    },
    /// Generate a random password (does not need the vault).
    #[command(name = "generate", aliases = ["gen"])]
    Generate {
        /// Number of characters.
        #[arg(default_value_t = 20)]
        size: usize,
        /// Generate without symbols.
        #[arg(long = "no-symbols")]
        no_symbols: bool,
        /// Copy instead of printing.
        #[arg(short = 'c', long = "copy")]
        copy: bool,
    },
    /// Change the master password (makes a new recovery key).
    #[command(name = "master-password", aliases = ["passwd"])]
    MasterPassword,
    /// Open the vault with the recovery key and set a new master password.
    #[command(name = "recover")]
    Recover,
    /// Set the sync folder (for example, inside Google Drive).
    #[command(name = "sync-folder")]
    SyncFolder {
        /// Folder; without it, shows the current one.
        folder: Option<PathBuf>,
        /// Turn syncing off.
        #[arg(long = "off")]
        turn_off: bool,
    },
    /// Sync now with the configured folder.
    #[command(name = "sync")]
    Sync,
    /// Import from CSV (Bitwarden, Chrome, KeePassXC, 1Password...) or KDBX.
    #[command(name = "import")]
    Import {
        /// File to import (.csv or .kdbx).
        file: PathBuf,
    },
    /// Export to KDBX (encrypted) or CSV (plain text, needs --plaintext).
    #[command(name = "export")]
    Export {
        /// Destination file (.kdbx or .csv).
        file: PathBuf,
        /// Confirm that the CSV will be unencrypted on disk.
        #[arg(long = "plaintext")]
        in_plain: bool,
        /// Overwrite the file if it exists.
        #[arg(long = "force")]
        force: bool,
    },
    /// Open the interactive screen: a table of accounts with reveal and copy.
    #[command(name = "ui")]
    Screen,
    /// Show where the files are and the sync status.
    #[command(name = "info")]
    Info,
}

type R<T> = Result<T, String>;

fn error(e: ApiError) -> String {
    match e {
        ApiError::Storage(_) if e.code() == "incorrect_credential_or_tampered_file" => {
            "incorrect password (or tampered file)".into()
        }
        _ => format!("{e} [{}]", e.code()),
    }
}

fn now() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn classes(no_symbols: bool) -> Classes {
    Classes {
        symbols: !no_symbols,
        ..Classes::ALL
    }
}

fn show_recovery(key: &neka_core::api::Key) -> R<()> {
    let text = Zeroizing::new(recovery_key_to_text(key));
    ui::recovery_key(text.as_str());
    ui::prompt("Press Enter once you have written it down.");
    terminal::line()?;
    Ok(())
}

/// Open session: vault, configuration and the password (for sync).
struct Session {
    cfg: Config,
    vault: Vault,
    password: Zeroizing<String>,
    sync: bool,
}

impl Session {
    fn open(cfg: Config, no_sync: bool) -> R<Self> {
        let path = cfg.vault();
        if !path.exists() {
            return Err(format!(
                "no vault in {}. Use `neka create` or `neka clone <file>`.",
                path.display()
            ));
        }
        let password = terminal::secret("Master password: ")?;
        let vault = Vault::open(
            &path,
            cfg.device_id,
            &Credential::Password(password.as_bytes()),
        )
        .map_err(error)?;
        let sync = !no_sync && cfg.sync_folder.is_some();
        let mut s = Session {
            cfg,
            vault,
            password,
            sync,
        };
        if s.sync {
            s.sync(false)?;
        }
        Ok(s)
    }

    /// Syncs. In automatic mode, a failure becomes a warning.
    fn sync(&mut self, explicit: bool) -> R<()> {
        let Some(folder) = self.cfg.sync_folder.clone() else {
            return Err("no sync folder. Use `neka sync-folder <folder>`.".into());
        };
        if let Some(u) = self.cfg.last_sync {
            if now() - u > 180 * 24 * 3600 * 1000 {
                ui::warning(
                    "this device has not synced for more than 180 days; entries deleted on another device may come back.",
                );
            }
        }
        let r = match self
            .vault
            .sync(&folder, self.password.as_bytes(), None, now())
        {
            Err(ApiError::PasswordChangedOnOtherDevice) => {
                ui::hint(
                    Flow::Error,
                    "The master password was changed on another device.",
                );
                let new = terminal::secret("New master password: ")?;
                let r = self
                    .vault
                    .sync(
                        &folder,
                        self.password.as_bytes(),
                        Some(new.as_bytes()),
                        now(),
                    )
                    .map_err(error)?;
                self.password = new;
                r
            }
            Err(e) if !explicit => {
                ui::warning(format!("sync failed: {}", error(e)));
                return Ok(());
            }
            r => r.map_err(error)?,
        };
        self.cfg.last_sync = Some(now());
        self.cfg.save()?;
        report_sync(&r, explicit);
        Ok(())
    }

    /// Writes and, if there is a folder, publishes.
    fn save(&mut self) -> R<()> {
        self.vault.save().map_err(error)?;
        if self.sync {
            self.sync(false)?;
        }
        Ok(())
    }
}

fn report_sync(r: &SyncReport, detailed: bool) {
    if r.adopted_new_password {
        ui::hint(
            Flow::Error,
            "This device's master password is now the new one. The valid recovery key is the one made when it was changed.",
        );
    }
    for p in &r.old_copies {
        ui::warning(format!(
            "{} is a copy with the old password; it was not merged. Delete it if you no longer need it.",
            p.display()
        ));
    }
    for p in &r.not_open {
        ui::warning(format!(
            "{} does not open with this password (password changed on two devices at the same time?). Change the password again on one of them and sync.",
            p.display()
        ));
    }
    if detailed {
        ui::success(
            Flow::Output,
            format!(
                "Synced: {} file(s) read, {} conflict copy(ies) absorbed{}.",
                r.merged.len(),
                r.removed.len(),
                if r.changed_locally {
                    ", local vault updated"
                } else {
                    ", nothing new"
                }
            ),
        );
        ui::hint(Flow::Output, format!("Backup: {}", r.backup.display()));
    }
}

/// Finds an entry by id (or a prefix of 4+ characters), exact title or
/// unique fragment of the title.
fn locate(vault: &Vault, r: &str) -> R<Uuid> {
    let es = vault.entries();
    let rl = r.to_lowercase();
    if rl.len() >= 4 {
        let v: Vec<&Entry> = es
            .iter()
            .filter(|e| e.id.simple().to_string().starts_with(&rl.replace('-', "")))
            .collect();
        if v.len() == 1 {
            return Ok(v[0].id);
        }
    }
    let exact: Vec<&Entry> = es.iter().filter(|e| e.title.to_lowercase() == rl).collect();
    if exact.len() == 1 {
        return Ok(exact[0].id);
    }
    let partial: Vec<&Entry> = if exact.is_empty() {
        es.iter()
            .filter(|e| e.title.to_lowercase().contains(&rl))
            .collect()
    } else {
        exact
    };
    match partial.len() {
        0 => Err(format!("no entry matching \"{r}\"")),
        1 => Ok(partial[0].id),
        _ => {
            let mut m = format!("\"{r}\" is ambiguous; use the id:\n");
            for e in partial {
                m.push_str(&list_line(e));
                m.push('\n');
            }
            Err(m.trim_end().to_owned())
        }
    }
}

fn short_id(id: &Uuid) -> String {
    id.simple().to_string()[..8].to_owned()
}

/// Plain-text line of an entry (used in error messages).
fn list_line(e: &Entry) -> String {
    let mut s = format!("{}  {}", short_id(&e.id), e.title);
    if !e.username.is_empty() {
        s.push_str(&format!("  ({})", e.username));
    }
    if !e.url.is_empty() {
        s.push_str(&format!("  {}", e.url));
    }
    if !e.tags.is_empty() {
        s.push_str(&format!("  [{}]", e.tags.join(", ")));
    }
    s
}

/// When the current password was set: the most recent change in the history
/// (ignoring versions equal to the current one) or, with no change, the creation of the entry.
fn password_date(e: &Entry) -> i64 {
    e.history
        .iter()
        .filter(|h| h.password != e.password)
        .map(|h| h.changed_at)
        .max()
        .unwrap_or(e.created_at)
}

fn data(ms: i64) -> String {
    // YYYY-MM-DD in UTC, without a calendar dependency.
    let days = ms.div_euclid(86_400_000);
    let z = days + 719_468;
    let era = z.div_euclid(146_097);
    let doe = z - era * 146_097;
    let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
    let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
    let mp = (5 * doy + 2) / 153;
    let d = doy - (153 * mp + 2) / 5 + 1;
    let m = if mp < 10 { mp + 3 } else { mp - 9 };
    let a = yoe + era * 400 + i64::from(m <= 2);
    format!("{a:04}-{m:02}-{d:02}")
}

fn run(cli: Cli) -> R<()> {
    let command = cli.command.unwrap_or(Command::Screen);
    let dir = cli
        .dir
        .clone()
        .or_else(config::default_folder)
        .ok_or("could not find the data folder; use --dir")?;

    if let Command::Generate {
        size,
        no_symbols,
        copy,
    } = command
    {
        let s = Zeroizing::new(
            generate_password(size, classes(no_symbols)).map_err(|e| e.to_string())?,
        );
        if copy {
            terminal::copy(&s, 20)?;
        } else {
            println!("{}", s.as_str());
        }
        return Ok(());
    }

    let mut cfg = Config::load(dir)?;

    match command {
        Command::Generate { .. } => unreachable!(),
        Command::Screen => {
            if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                return Err("the interactive screen needs a terminal; see `neka --help`".into());
            }
            let has_vault = cfg.vault().exists();
            let mut service = service::VaultService::new(cfg, !cli.no_sync);
            tui::run(has_vault, &mut service)
        }
        Command::Info => {
            ui::title(Flow::Output, "neka");
            ui::wide_field("Data folder:", 13, cfg.dir.display());
            ui::wide_field(
                "Vault:",
                13,
                format!(
                    "{}{}",
                    cfg.vault().display(),
                    if cfg.vault().exists() {
                        ""
                    } else {
                        " (does not exist)"
                    }
                ),
            );
            ui::wide_field("Device:", 13, cfg.device_id);
            match &cfg.sync_folder {
                Some(p) => ui::wide_field("Sync folder:", 13, p.display()),
                None => ui::wide_field("Sync folder:", 13, "none"),
            }
            if let Some(u) = cfg.last_sync {
                ui::wide_field("Last sync:", 13, data(u));
            }
            Ok(())
        }
        Command::Create => {
            let path = cfg.vault();
            if path.exists() {
                return Err(format!("a vault already exists in {}", path.display()));
            }
            let password = terminal::new_password("New master password")?;
            ui::hint(
                Flow::Error,
                "Creating the vault (deriving the key takes about a second)...",
            );
            let (_c, rec) = Vault::create(
                &path,
                cfg.device_id,
                password.as_bytes(),
                KdfParams::DEFAULT,
            )
            .map_err(error)?;
            ui::success(Flow::Output, format!("Vault created in {}", path.display()));
            show_recovery(&rec)?;
            ui::hint(
                Flow::Output,
                "Next step: `neka sync-folder \"<folder in Google Drive>\"` to start syncing.",
            );
            Ok(())
        }
        Command::Clone { file } => {
            let path = cfg.vault();
            let password = terminal::secret("Master password: ")?;
            Vault::clone(&file, &path, cfg.device_id, password.as_bytes()).map_err(error)?;
            if cfg.sync_folder.is_none() {
                cfg.sync_folder = file.parent().map(Path::to_owned);
                cfg.save()?;
            }
            ui::success(
                Flow::Output,
                format!("Vault brought to {}.", path.display()),
            );
            if let Some(p) = &cfg.sync_folder {
                ui::hint(Flow::Output, format!("Sync folder: {}", p.display()));
            }
            Ok(())
        }
        Command::SyncFolder { folder, turn_off } => {
            if turn_off {
                cfg.sync_folder = None;
                cfg.save()?;
                ui::success(Flow::Output, "Syncing turned off.");
                return Ok(());
            }
            match folder {
                None => match &cfg.sync_folder {
                    Some(p) => println!("{}", p.display()),
                    None => println!("none"),
                },
                Some(p) => {
                    if !p.is_dir() {
                        return Err(format!("{} is not a folder", p.display()));
                    }
                    let p = p.canonicalize().unwrap_or(p);
                    cfg.sync_folder = Some(p.clone());
                    cfg.save()?;
                    ui::success(Flow::Output, format!("Sync folder: {}", p.display()));
                    ui::hint(
                        Flow::Output,
                        "Tip: in Google Drive for desktop, mark the folder as \"Available offline\".",
                    );
                }
            }
            Ok(())
        }
        Command::Sync => {
            let mut s = Session::open(cfg, true)?;
            s.sync(true)
        }
        Command::Add {
            title,
            username,
            url,
            notes,
            tags,
            generate,
            no_symbols,
        } => {
            let mut s = Session::open(cfg, cli.no_sync)?;
            let password = match generate {
                Some(n) => Zeroizing::new(
                    generate_password(n, classes(no_symbols)).map_err(|e| e.to_string())?,
                ),
                None => terminal::secret("Account password (Enter for none): ")?,
            };
            let id = s
                .vault
                .add(
                    NewEntry {
                        title,
                        username: username.unwrap_or_default(),
                        password: password.to_string(),
                        url: url.unwrap_or_default(),
                        notes: notes.unwrap_or_default(),
                        tags,
                        ..Default::default()
                    },
                    now(),
                )
                .map_err(error)?;
            s.save()?;
            ui::success(Flow::Output, format!("Added: {}", short_id(&id)));
            if generate.is_some() {
                ui::hint(
                    Flow::Error,
                    format!(
                        "Password generated. Use `neka copy {}` to copy it.",
                        short_id(&id)
                    ),
                );
            }
            Ok(())
        }
        Command::List {
            search,
            tag,
            reveal,
        } => {
            let s = Session::open(cfg, cli.no_sync)?;
            let b = search.map(|b| b.to_lowercase());
            let mut es: Vec<&Entry> = s
                .vault
                .entries()
                .iter()
                .filter(|e| {
                    b.as_ref().is_none_or(|b| {
                        e.title.to_lowercase().contains(b)
                            || e.username.to_lowercase().contains(b)
                            || e.url.to_lowercase().contains(b)
                    })
                })
                .filter(|e| {
                    tag.as_ref()
                        .is_none_or(|t| e.tags.iter().any(|x| x.eq_ignore_ascii_case(t)))
                })
                .collect();
            es.sort_by_key(|e| e.title.to_lowercase());
            if ui::decorated(Flow::Output) {
                if !es.is_empty() {
                    let lines: Vec<ui::TableRow> = es
                        .iter()
                        .map(|e| ui::TableRow {
                            id: short_id(&e.id),
                            title: e.title.clone(),
                            username: e.username.clone(),
                            password_empty: e.password.is_empty(),
                            password: reveal.then(|| e.password.clone()),
                            password_date: data(password_date(e)),
                            tags: e.tags.clone(),
                        })
                        .collect();
                    ui::entry_table(&lines, reveal);
                }
            } else {
                for e in &es {
                    ui::entry_line(&short_id(&e.id), &e.title, &e.username, &e.url, &e.tags);
                    if reveal {
                        println!("    password: {}", e.password);
                        println!("    set on: {}", data(password_date(e)));
                    }
                }
            }
            if reveal {
                ui::warning("passwords are now visible in this terminal's scrollback.");
            }
            ui::footer_list(es.len());
            Ok(())
        }
        Command::Show {
            entry,
            password,
            history,
        } => {
            let s = Session::open(cfg, cli.no_sync)?;
            let e = s.vault.entry(locate(&s.vault, &entry)?).map_err(error)?;
            ui::title(Flow::Output, &e.title);
            ui::field("Id:", e.id);
            ui::field("Title:", &e.title);
            ui::field("User:", &e.username);
            if password {
                ui::field("Password:", &e.password);
            } else {
                ui::field(
                    "Password:",
                    if e.password.is_empty() {
                        "(empty)"
                    } else {
                        "******** (use --password or `neka copy`)"
                    },
                );
            }
            if !e.url.is_empty() {
                ui::field("URL:", &e.url);
            }
            if !e.tags.is_empty() {
                ui::field("Tags:", e.tags.join(", "));
            }
            for (k, v) in &e.extra {
                ui::field(
                    &format!("{k}:"),
                    if password { v.as_str() } else { "********" },
                );
            }
            if !e.notes.is_empty() {
                ui::line(
                    Flow::Output,
                    ui::tone(Flow::Output, Tone::Secondary, true, "Notes:"),
                );
                ui::line(Flow::Output, &e.notes);
            }
            ui::field("Set on:", data(password_date(e)));
            ui::line(
                Flow::Output,
                ui::erased(format!(
                    "Created {} · changed {}",
                    data(e.created_at),
                    data(e.updated_at)
                )),
            );
            let old: Vec<_> = e
                .history
                .iter()
                .rev()
                .filter(|h| h.password != e.password)
                .collect();
            if history {
                for h in &old {
                    ui::line(
                        Flow::Output,
                        format!(
                            "  {} {}",
                            ui::erased(format!("old ({}):", data(h.changed_at))),
                            h.password
                        ),
                    );
                }
            } else if !old.is_empty() {
                ui::hint(
                    Flow::Output,
                    format!("{} old password(s) (use --history).", old.len()),
                );
            }
            Ok(())
        }
        Command::Copy {
            entry,
            username,
            seconds,
        } => {
            let s = Session::open(cfg, cli.no_sync)?;
            let e = s.vault.entry(locate(&s.vault, &entry)?).map_err(error)?;
            let text = Zeroizing::new(if username {
                e.username.clone()
            } else {
                e.password.clone()
            });
            ui::hint(Flow::Error, &e.title);
            drop(s); // releases the vault lock before waiting
            terminal::copy(&text, seconds)
        }
        Command::Edit {
            entry,
            title,
            username,
            url,
            notes,
            tags,
            no_tags,
            password,
            generate,
            no_symbols,
        } => {
            let mut s = Session::open(cfg, cli.no_sync)?;
            let id = locate(&s.vault, &entry)?;
            let new = match (generate, password) {
                (Some(n), _) => Some(Zeroizing::new(
                    generate_password(n, classes(no_symbols)).map_err(|e| e.to_string())?,
                )),
                (None, true) => Some(terminal::secret("New account password: ")?),
                _ => None,
            };
            s.vault
                .edit(
                    id,
                    |e| {
                        if let Some(t) = title {
                            e.title = t;
                        }
                        if let Some(u) = username {
                            e.username = u;
                        }
                        if let Some(u) = url {
                            e.url = u;
                        }
                        if let Some(n) = notes {
                            e.notes = n;
                        }
                        if no_tags {
                            e.tags.clear();
                        } else if !tags.is_empty() {
                            e.tags = tags;
                        }
                        if let Some(p) = &new {
                            e.password = p.to_string();
                        }
                    },
                    now(),
                )
                .map_err(error)?;
            s.save()?;
            ui::success(Flow::Output, format!("Edited: {}", short_id(&id)));
            Ok(())
        }
        Command::Remove { entry, sim } => {
            let mut s = Session::open(cfg, cli.no_sync)?;
            let id = locate(&s.vault, &entry)?;
            let t = s.vault.entry(id).map_err(error)?.title.clone();
            if !sim && !terminal::confirm(&format!("Remove \"{t}\"?"))? {
                println!("Nothing removed.");
                return Ok(());
            }
            s.vault.remove(id, now()).map_err(error)?;
            s.save()?;
            ui::success(Flow::Output, format!("Removed: {t}"));
            Ok(())
        }
        Command::MasterPassword => {
            let mut s = Session::open(cfg, cli.no_sync)?;
            let new = terminal::new_password("New master password")?;
            let rec = s
                .vault
                .change_master_password(new.as_bytes(), KdfParams::DEFAULT)
                .map_err(error)?;
            s.password = new;
            s.save()?;
            ui::success(
                Flow::Output,
                "Master password changed. The old recovery key is no longer valid.",
            );
            show_recovery(&rec)?;
            if s.sync {
                ui::hint(
                    Flow::Output,
                    "On the other devices, the next sync will ask for the new password.",
                );
            }
            Ok(())
        }
        Command::Recover => {
            let path = cfg.vault();
            let text = terminal::secret("Recovery key: ")?;
            let mut c =
                Vault::open(&path, cfg.device_id, &Credential::Recovery(&text)).map_err(error)?;
            let new = terminal::new_password("New master password")?;
            let rec = c
                .change_master_password(new.as_bytes(), KdfParams::DEFAULT)
                .map_err(error)?;
            c.save().map_err(error)?;
            ui::success(Flow::Output, "Master password reset.");
            show_recovery(&rec)?;
            if cfg.sync_folder.is_some() && !cli.no_sync {
                let mut s = Session {
                    cfg,
                    vault: c,
                    password: new,
                    sync: true,
                };
                s.sync(false)?;
            }
            Ok(())
        }
        Command::Import { file } => {
            let bytes = Zeroizing::new(
                std::fs::read(&file).map_err(|_| format!("could not read {}", file.display()))?,
            );
            let kdbx = bytes.starts_with(&[0x03, 0xd9, 0xa2, 0x9a]);
            let records = if kdbx {
                let sk = terminal::secret("KeePass file password: ")?;
                read_kdbx(&bytes, &sk).map_err(|e| e.to_string())?
            } else {
                read_csv(&bytes).map_err(|e| e.to_string())?
            };
            let mut s = Session::open(cfg, cli.no_sync)?;
            let r = s.vault.import(&records, now()).map_err(error)?;
            s.save()?;
            ui::success(
                Flow::Output,
                format!(
                    "Imported: {} · already existed: {} · rejected (field too large): {}",
                    r.added, r.duplicates, r.invalid
                ),
            );
            if !kdbx {
                ui::warning("remember to delete the source CSV: it is in plain text.");
            }
            Ok(())
        }
        Command::Export {
            file,
            in_plain,
            force,
        } => {
            if file.exists() && !force {
                return Err(format!("{} already exists (use --force)", file.display()));
            }
            let ext = file
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_lowercase)
                .unwrap_or_default();
            if ext != "kdbx" && ext != "csv" {
                return Err("use the .kdbx or .csv extension".into());
            }
            if ext == "csv" && !in_plain {
                return Err(
                    "a CSV is unencrypted on disk. Confirm with --plaintext or prefer .kdbx."
                        .into(),
                );
            }
            let s = Session::open(cfg, cli.no_sync)?;
            let regs = s.vault.records();
            let bytes = if ext == "kdbx" {
                let sk = terminal::new_password("KeePass file password")?;
                ui::hint(Flow::Error, "Generating the KDBX...");
                generate_kdbx(&regs, &sk).map_err(|e| e.to_string())?
            } else {
                generate_csv(&regs).map_err(|e| e.to_string())?
            };
            let bytes = Zeroizing::new(bytes);
            std::fs::write(&file, bytes.as_slice())
                .map_err(|_| format!("could not write {}", file.display()))?;
            ui::success(
                Flow::Output,
                format!("Exported {} entry(ies) to {}.", regs.len(), file.display()),
            );
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match run(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(m) => {
            ui::failure(m);
            ExitCode::FAILURE
        }
    }
}
