//! `neka`: interface de linha de comando. Só argumentos, prompts, área de
//! transferência e formatação; as regras ficam em `neka_core::api`.

mod config;
mod terminal;
mod tui;
mod ui;

use std::path::{Path, PathBuf};
use std::process::ExitCode;
use std::time::{SystemTime, UNIX_EPOCH};

use clap::{Parser, Subcommand};
use neka_core::api::{
    chave_de_recuperacao_para_texto, gerar_csv, gerar_kdbx, gerar_senha, ler_csv, ler_kdbx,
    Classes, Cofre, Credencial, Entrada, ErroApi, NovaEntrada, ParametrosKdf, RelatorioSync, Uuid,
    VERSAO,
};
use zeroize::Zeroizing;

use config::Config;
use ui::{Fluxo, Tom};

/// A cozy password manager for your terminal. The vault is a single encrypted
/// file and can sync through a Google Drive folder.
#[derive(Parser)]
#[command(name = "neka", version = VERSAO, about, styles = ui::estilos_clap())]
struct Cli {
    /// Data folder (default: %APPDATA%\neka on Windows).
    #[arg(long, global = true, value_name = "FOLDER")]
    dir: Option<PathBuf>,
    /// Do not sync automatically in this command.
    #[arg(long = "no-sync", alias = "sem-sync", global = true)]
    sem_sync: bool,
    #[command(subcommand)]
    comando: Comando,
}

#[derive(Subcommand)]
enum Comando {
    /// Create a new vault on this device.
    #[command(name = "create", aliases = ["init", "criar"])]
    Criar,
    /// Bring a vault that is already in the cloud folder to this device.
    #[command(name = "clone", alias = "clonar")]
    Clonar {
        /// .neka file in the synced folder.
        arquivo: PathBuf,
    },
    /// Add an entry. The password is asked without echo, or generated with --generate.
    #[command(name = "add", alias = "adicionar")]
    Adicionar {
        /// Entry title.
        titulo: String,
        /// Username or e-mail.
        #[arg(short = 'u', long = "user", alias = "usuario")]
        usuario: Option<String>,
        /// Address (URL) of the site or service.
        #[arg(long)]
        url: Option<String>,
        /// Free-form notes.
        #[arg(long = "notes", alias = "notas")]
        notas: Option<String>,
        /// Tag (can be repeated).
        #[arg(short = 't', long = "tag", alias = "etiqueta")]
        etiquetas: Vec<String>,
        /// Generate a password with N characters (default 20).
        #[arg(short = 'g', long = "generate", alias = "gerar", value_name = "N", num_args = 0..=1, default_missing_value = "20")]
        gerar: Option<usize>,
        /// Generate without symbols (for sites that reject them).
        #[arg(long = "no-symbols", alias = "sem-simbolos")]
        sem_simbolos: bool,
    },
    /// List entries, optionally filtered by text or tag.
    #[command(name = "list", aliases = ["ls", "listar"])]
    Listar {
        /// Text to look for in title, username or address.
        busca: Option<String>,
        /// Only entries with this tag.
        #[arg(short = 't', long = "tag", alias = "etiqueta")]
        etiqueta: Option<String>,
        /// Unlock the view: show the passwords in the list (they stay in the
        /// terminal scrollback).
        #[arg(short = 'r', long = "reveal")]
        revelar: bool,
    },
    /// Show an entry (the password only with --password).
    #[command(name = "show", alias = "mostrar")]
    Mostrar {
        /// Entry id (4+ characters) or title.
        entrada: String,
        /// Reveal the password.
        #[arg(short = 'p', long = "password", alias = "senha", short_alias = 's')]
        senha: bool,
        /// Also show old passwords.
        #[arg(long = "history", alias = "historico")]
        historico: bool,
    },
    /// Copy the password (or the username) to the clipboard.
    #[command(name = "copy", aliases = ["cp", "copiar"])]
    Copiar {
        /// Entry id (4+ characters) or title.
        entrada: String,
        /// Copy the username instead of the password.
        #[arg(short = 'u', long = "user", alias = "usuario")]
        usuario: bool,
        /// Seconds until the clipboard is cleared (0 = never clear).
        #[arg(long = "seconds", alias = "segundos", default_value_t = 20)]
        segundos: u64,
    },
    /// Edit an entry. --password asks for a new password; --generate makes one.
    #[command(name = "edit", alias = "editar")]
    Editar {
        /// Entry id (4+ characters) or title.
        entrada: String,
        /// New title.
        #[arg(long = "title", alias = "titulo")]
        titulo: Option<String>,
        /// New username.
        #[arg(short = 'u', long = "user", alias = "usuario")]
        usuario: Option<String>,
        /// New address (URL).
        #[arg(long)]
        url: Option<String>,
        /// New notes.
        #[arg(long = "notes", alias = "notas")]
        notas: Option<String>,
        /// Replace the tags (can be repeated).
        #[arg(short = 't', long = "tag", alias = "etiqueta")]
        etiquetas: Vec<String>,
        /// Remove all tags.
        #[arg(long = "no-tags", alias = "sem-etiquetas")]
        sem_etiquetas: bool,
        /// Ask for a new password.
        #[arg(short = 'p', long = "password", alias = "senha", short_alias = 's')]
        senha: bool,
        /// Generate a new password with N characters (default 20).
        #[arg(short = 'g', long = "generate", alias = "gerar", value_name = "N", num_args = 0..=1, default_missing_value = "20")]
        gerar: Option<usize>,
        /// Generate without symbols.
        #[arg(long = "no-symbols", alias = "sem-simbolos")]
        sem_simbolos: bool,
    },
    /// Remove an entry.
    #[command(name = "remove", aliases = ["rm", "remover"])]
    Remover {
        /// Entry id (4+ characters) or title.
        entrada: String,
        /// Do not ask for confirmation.
        #[arg(short = 'y', long = "yes", alias = "sim", short_alias = 's')]
        sim: bool,
    },
    /// Generate a random password (does not need the vault).
    #[command(name = "generate", aliases = ["gen", "gerar"])]
    Gerar {
        /// Number of characters.
        #[arg(default_value_t = 20)]
        tamanho: usize,
        /// Generate without symbols.
        #[arg(long = "no-symbols", alias = "sem-simbolos")]
        sem_simbolos: bool,
        /// Copy instead of printing.
        #[arg(short = 'c', long = "copy", alias = "copiar")]
        copiar: bool,
    },
    /// Change the master password (makes a new recovery key).
    #[command(name = "master-password", aliases = ["passwd", "senha-mestra"])]
    SenhaMestra,
    /// Open the vault with the recovery key and set a new master password.
    #[command(name = "recover", alias = "recuperar")]
    Recuperar,
    /// Set the sync folder (for example, inside Google Drive).
    #[command(name = "sync-folder", alias = "pasta-sync")]
    PastaSync {
        /// Folder; without it, shows the current one.
        pasta: Option<PathBuf>,
        /// Turn syncing off.
        #[arg(long = "off", alias = "desligar")]
        desligar: bool,
    },
    /// Sync now with the configured folder.
    #[command(name = "sync", alias = "sincronizar")]
    Sincronizar,
    /// Import from CSV (Bitwarden, Chrome, KeePassXC, 1Password...) or KDBX.
    #[command(name = "import", alias = "importar")]
    Importar {
        /// File to import (.csv or .kdbx).
        arquivo: PathBuf,
    },
    /// Export to KDBX (encrypted) or CSV (plain text, needs --plaintext).
    #[command(name = "export", alias = "exportar")]
    Exportar {
        /// Destination file (.kdbx or .csv).
        arquivo: PathBuf,
        /// Confirm that the CSV will be unencrypted on disk.
        #[arg(long = "plaintext", alias = "em-claro")]
        em_claro: bool,
        /// Overwrite the file if it exists.
        #[arg(long = "force", alias = "forcar")]
        forcar: bool,
    },
    /// Open the interactive screen: a table of accounts with reveal and copy.
    #[command(name = "ui", alias = "tui")]
    Tela,
    /// Show where the files are and the sync status.
    #[command(name = "info")]
    Info,
}

type R<T> = Result<T, String>;

fn erro(e: ErroApi) -> String {
    match e {
        ErroApi::Armazenamento(_) if e.codigo() == "incorrect_credential_or_tampered_file" => {
            "incorrect password (or tampered file)".into()
        }
        _ => format!("{e} [{}]", e.codigo()),
    }
}

fn agora() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_millis() as i64)
        .unwrap_or(0)
}

fn classes(sem_simbolos: bool) -> Classes {
    Classes {
        simbolos: !sem_simbolos,
        ..Classes::TODAS
    }
}

fn mostrar_recuperacao(chave: &neka_core::api::Chave) -> R<()> {
    let texto = Zeroizing::new(chave_de_recuperacao_para_texto(chave));
    ui::chave_de_recuperacao(texto.as_str());
    ui::prompt("Press Enter once you have written it down.");
    terminal::linha()?;
    Ok(())
}

/// Sessão aberta: cofre, configuração e a senha (para a sincronização).
struct Sessao {
    cfg: Config,
    cofre: Cofre,
    senha: Zeroizing<String>,
    sync: bool,
}

impl Sessao {
    fn abrir(cfg: Config, sem_sync: bool) -> R<Self> {
        let caminho = cfg.cofre();
        if !caminho.exists() {
            return Err(format!(
                "no vault in {}. Use `neka create` or `neka clone <file>`.",
                caminho.display()
            ));
        }
        let senha = terminal::segredo("Master password: ")?;
        let cofre = Cofre::abrir(
            &caminho,
            cfg.device_id,
            &Credencial::Senha(senha.as_bytes()),
        )
        .map_err(erro)?;
        let sync = !sem_sync && cfg.pasta_sync.is_some();
        let mut s = Sessao {
            cfg,
            cofre,
            senha,
            sync,
        };
        if s.sync {
            s.sincronizar(false)?;
        }
        Ok(s)
    }

    /// Sincroniza. Em modo automático, uma falha vira aviso.
    fn sincronizar(&mut self, explicito: bool) -> R<()> {
        let Some(pasta) = self.cfg.pasta_sync.clone() else {
            return Err("no sync folder. Use `neka sync-folder <folder>`.".into());
        };
        if let Some(u) = self.cfg.ultima_sync {
            if agora() - u > 180 * 24 * 3600 * 1000 {
                ui::aviso(
                    "this device has not synced for more than 180 days; entries deleted on another device may come back.",
                );
            }
        }
        let r = match self
            .cofre
            .sincronizar(&pasta, self.senha.as_bytes(), None, agora())
        {
            Err(ErroApi::SenhaTrocadaEmOutroDispositivo) => {
                ui::dica(
                    Fluxo::Erro,
                    "The master password was changed on another device.",
                );
                let nova = terminal::segredo("New master password: ")?;
                let r = self
                    .cofre
                    .sincronizar(
                        &pasta,
                        self.senha.as_bytes(),
                        Some(nova.as_bytes()),
                        agora(),
                    )
                    .map_err(erro)?;
                self.senha = nova;
                r
            }
            Err(e) if !explicito => {
                ui::aviso(format!("sync failed: {}", erro(e)));
                return Ok(());
            }
            r => r.map_err(erro)?,
        };
        self.cfg.ultima_sync = Some(agora());
        self.cfg.salvar()?;
        relatar_sync(&r, explicito);
        Ok(())
    }

    /// Grava e, se houver pasta, publica.
    fn salvar(&mut self) -> R<()> {
        self.cofre.salvar().map_err(erro)?;
        if self.sync {
            self.sincronizar(false)?;
        }
        Ok(())
    }
}

fn relatar_sync(r: &RelatorioSync, detalhado: bool) {
    if r.adotou_senha_nova {
        ui::dica(
            Fluxo::Erro,
            "This device's master password is now the new one. The valid recovery key is the one made when it was changed.",
        );
    }
    for p in &r.copias_velhas {
        ui::aviso(format!(
            "{} is a copy with the old password; it was not merged. Delete it if you no longer need it.",
            p.display()
        ));
    }
    for p in &r.nao_abertos {
        ui::aviso(format!(
            "{} does not open with this password (password changed on two devices at the same time?). Change the password again on one of them and sync.",
            p.display()
        ));
    }
    if detalhado {
        ui::sucesso(
            Fluxo::Saida,
            format!(
                "Synced: {} file(s) read, {} conflict copy(ies) absorbed{}.",
                r.mesclados.len(),
                r.removidos.len(),
                if r.alterou_local {
                    ", local vault updated"
                } else {
                    ", nothing new"
                }
            ),
        );
        ui::dica(Fluxo::Saida, format!("Backup: {}", r.backup.display()));
    }
}

/// Acha uma entrada por id (ou prefixo de 4+ caracteres), título exato ou
/// trecho único do título.
fn achar(cofre: &Cofre, r: &str) -> R<Uuid> {
    let es = cofre.entradas();
    let rl = r.to_lowercase();
    if rl.len() >= 4 {
        let v: Vec<&Entrada> = es
            .iter()
            .filter(|e| e.id.simple().to_string().starts_with(&rl.replace('-', "")))
            .collect();
        if v.len() == 1 {
            return Ok(v[0].id);
        }
    }
    let exatos: Vec<&Entrada> = es.iter().filter(|e| e.title.to_lowercase() == rl).collect();
    if exatos.len() == 1 {
        return Ok(exatos[0].id);
    }
    let parciais: Vec<&Entrada> = if exatos.is_empty() {
        es.iter()
            .filter(|e| e.title.to_lowercase().contains(&rl))
            .collect()
    } else {
        exatos
    };
    match parciais.len() {
        0 => Err(format!("no entry matching \"{r}\"")),
        1 => Ok(parciais[0].id),
        _ => {
            let mut m = format!("\"{r}\" is ambiguous; use the id:\n");
            for e in parciais {
                m.push_str(&linha_lista(e));
                m.push('\n');
            }
            Err(m.trim_end().to_owned())
        }
    }
}

fn id_curto(id: &Uuid) -> String {
    id.simple().to_string()[..8].to_owned()
}

/// Linha de texto puro de uma entrada (usada em mensagens de erro).
fn linha_lista(e: &Entrada) -> String {
    let mut s = format!("{}  {}", id_curto(&e.id), e.title);
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

/// Quando a senha atual foi definida: a troca mais recente no histórico
/// (ignorando versões iguais à atual) ou, sem troca, a criação da entrada.
fn data_da_senha(e: &Entrada) -> i64 {
    e.history
        .iter()
        .filter(|h| h.password != e.password)
        .map(|h| h.changed_at)
        .max()
        .unwrap_or(e.created_at)
}

fn data(ms: i64) -> String {
    // AAAA-MM-DD em UTC, sem dependência de calendário.
    let dias = ms.div_euclid(86_400_000);
    let z = dias + 719_468;
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

fn executar(cli: Cli) -> R<()> {
    let dir = cli
        .dir
        .clone()
        .or_else(config::pasta_padrao)
        .ok_or("could not find the data folder; use --dir")?;

    if let Comando::Gerar {
        tamanho,
        sem_simbolos,
        copiar,
    } = cli.comando
    {
        let s =
            Zeroizing::new(gerar_senha(tamanho, classes(sem_simbolos)).map_err(|e| e.to_string())?);
        if copiar {
            terminal::copiar(&s, 20)?;
        } else {
            println!("{}", s.as_str());
        }
        return Ok(());
    }

    let mut cfg = Config::carregar(dir)?;

    match cli.comando {
        Comando::Gerar { .. } => unreachable!(),
        Comando::Tela => {
            if !std::io::IsTerminal::is_terminal(&std::io::stdout()) {
                return Err("the interactive screen needs a terminal".into());
            }
            let s = Sessao::abrir(cfg, cli.sem_sync)?;
            let mut es: Vec<&Entrada> = s.cofre.entradas().iter().collect();
            es.sort_by_key(|e| e.title.to_lowercase());
            let filas = es
                .iter()
                .map(|e| tui::Fila {
                    id: e.id.to_string(),
                    titulo: e.title.clone(),
                    usuario: e.username.clone(),
                    url: e.url.clone(),
                    notas: e.notes.clone(),
                    senha: Zeroizing::new(e.password.clone()),
                    data_senha: data(data_da_senha(e)),
                    etiquetas: e.tags.clone(),
                })
                .collect();
            drop(s);
            tui::executar(filas)
        }
        Comando::Info => {
            ui::titulo(Fluxo::Saida, "neka");
            ui::campo_largo("Data folder:", 13, cfg.dir.display());
            ui::campo_largo(
                "Vault:",
                13,
                format!(
                    "{}{}",
                    cfg.cofre().display(),
                    if cfg.cofre().exists() {
                        ""
                    } else {
                        " (does not exist)"
                    }
                ),
            );
            ui::campo_largo("Device:", 13, cfg.device_id);
            match &cfg.pasta_sync {
                Some(p) => ui::campo_largo("Sync folder:", 13, p.display()),
                None => ui::campo_largo("Sync folder:", 13, "none"),
            }
            if let Some(u) = cfg.ultima_sync {
                ui::campo_largo("Last sync:", 13, data(u));
            }
            Ok(())
        }
        Comando::Criar => {
            let caminho = cfg.cofre();
            if caminho.exists() {
                return Err(format!("a vault already exists in {}", caminho.display()));
            }
            let senha = terminal::senha_nova("New master password")?;
            ui::dica(
                Fluxo::Erro,
                "Creating the vault (deriving the key takes about a second)...",
            );
            let (_c, rec) = Cofre::criar(
                &caminho,
                cfg.device_id,
                senha.as_bytes(),
                ParametrosKdf::PADRAO,
            )
            .map_err(erro)?;
            ui::sucesso(
                Fluxo::Saida,
                format!("Vault created in {}", caminho.display()),
            );
            mostrar_recuperacao(&rec)?;
            ui::dica(
                Fluxo::Saida,
                "Next step: `neka sync-folder \"<folder in Google Drive>\"` to start syncing.",
            );
            Ok(())
        }
        Comando::Clonar { arquivo } => {
            let caminho = cfg.cofre();
            let senha = terminal::segredo("Master password: ")?;
            Cofre::clonar(&arquivo, &caminho, cfg.device_id, senha.as_bytes()).map_err(erro)?;
            if cfg.pasta_sync.is_none() {
                cfg.pasta_sync = arquivo.parent().map(Path::to_owned);
                cfg.salvar()?;
            }
            ui::sucesso(
                Fluxo::Saida,
                format!("Vault brought to {}.", caminho.display()),
            );
            if let Some(p) = &cfg.pasta_sync {
                ui::dica(Fluxo::Saida, format!("Sync folder: {}", p.display()));
            }
            Ok(())
        }
        Comando::PastaSync { pasta, desligar } => {
            if desligar {
                cfg.pasta_sync = None;
                cfg.salvar()?;
                ui::sucesso(Fluxo::Saida, "Syncing turned off.");
                return Ok(());
            }
            match pasta {
                None => match &cfg.pasta_sync {
                    Some(p) => println!("{}", p.display()),
                    None => println!("none"),
                },
                Some(p) => {
                    if !p.is_dir() {
                        return Err(format!("{} is not a folder", p.display()));
                    }
                    let p = p.canonicalize().unwrap_or(p);
                    cfg.pasta_sync = Some(p.clone());
                    cfg.salvar()?;
                    ui::sucesso(Fluxo::Saida, format!("Sync folder: {}", p.display()));
                    ui::dica(
                        Fluxo::Saida,
                        "Tip: in Google Drive for desktop, mark the folder as \"Available offline\".",
                    );
                }
            }
            Ok(())
        }
        Comando::Sincronizar => {
            let mut s = Sessao::abrir(cfg, true)?;
            s.sincronizar(true)
        }
        Comando::Adicionar {
            titulo,
            usuario,
            url,
            notas,
            etiquetas,
            gerar,
            sem_simbolos,
        } => {
            let mut s = Sessao::abrir(cfg, cli.sem_sync)?;
            let senha = match gerar {
                Some(n) => Zeroizing::new(
                    gerar_senha(n, classes(sem_simbolos)).map_err(|e| e.to_string())?,
                ),
                None => terminal::segredo("Account password (Enter for none): ")?,
            };
            let id = s
                .cofre
                .adicionar(
                    NovaEntrada {
                        title: titulo,
                        username: usuario.unwrap_or_default(),
                        password: senha.to_string(),
                        url: url.unwrap_or_default(),
                        notes: notas.unwrap_or_default(),
                        tags: etiquetas,
                        ..Default::default()
                    },
                    agora(),
                )
                .map_err(erro)?;
            s.salvar()?;
            ui::sucesso(Fluxo::Saida, format!("Added: {}", id_curto(&id)));
            if gerar.is_some() {
                ui::dica(
                    Fluxo::Erro,
                    format!(
                        "Password generated. Use `neka copy {}` to copy it.",
                        id_curto(&id)
                    ),
                );
            }
            Ok(())
        }
        Comando::Listar {
            busca,
            etiqueta,
            revelar,
        } => {
            let s = Sessao::abrir(cfg, cli.sem_sync)?;
            let b = busca.map(|b| b.to_lowercase());
            let mut es: Vec<&Entrada> = s
                .cofre
                .entradas()
                .iter()
                .filter(|e| {
                    b.as_ref().is_none_or(|b| {
                        e.title.to_lowercase().contains(b)
                            || e.username.to_lowercase().contains(b)
                            || e.url.to_lowercase().contains(b)
                    })
                })
                .filter(|e| {
                    etiqueta
                        .as_ref()
                        .is_none_or(|t| e.tags.iter().any(|x| x.eq_ignore_ascii_case(t)))
                })
                .collect();
            es.sort_by_key(|e| e.title.to_lowercase());
            if ui::enfeitado(Fluxo::Saida) {
                if !es.is_empty() {
                    let linhas: Vec<ui::LinhaTabela> = es
                        .iter()
                        .map(|e| ui::LinhaTabela {
                            id: id_curto(&e.id),
                            titulo: e.title.clone(),
                            usuario: e.username.clone(),
                            senha_vazia: e.password.is_empty(),
                            senha: revelar.then(|| e.password.clone()),
                            data_senha: data(data_da_senha(e)),
                            etiquetas: e.tags.clone(),
                        })
                        .collect();
                    ui::tabela_entradas(&linhas, revelar);
                }
            } else {
                for e in &es {
                    ui::linha_entrada(&id_curto(&e.id), &e.title, &e.username, &e.url, &e.tags);
                    if revelar {
                        println!("    password: {}", e.password);
                        println!("    set on: {}", data(data_da_senha(e)));
                    }
                }
            }
            if revelar {
                ui::aviso("passwords are now visible in this terminal's scrollback.");
            }
            ui::rodape_lista(es.len());
            Ok(())
        }
        Comando::Mostrar {
            entrada,
            senha,
            historico,
        } => {
            let s = Sessao::abrir(cfg, cli.sem_sync)?;
            let e = s.cofre.entrada(achar(&s.cofre, &entrada)?).map_err(erro)?;
            ui::titulo(Fluxo::Saida, &e.title);
            ui::campo("Id:", e.id);
            ui::campo("Title:", &e.title);
            ui::campo("User:", &e.username);
            if senha {
                ui::campo("Password:", &e.password);
            } else {
                ui::campo(
                    "Password:",
                    if e.password.is_empty() {
                        "(empty)"
                    } else {
                        "******** (use --password or `neka copy`)"
                    },
                );
            }
            if !e.url.is_empty() {
                ui::campo("URL:", &e.url);
            }
            if !e.tags.is_empty() {
                ui::campo("Tags:", e.tags.join(", "));
            }
            for (k, v) in &e.extra {
                ui::campo(
                    &format!("{k}:"),
                    if senha { v.as_str() } else { "********" },
                );
            }
            if !e.notes.is_empty() {
                ui::linha(
                    Fluxo::Saida,
                    ui::tom(Fluxo::Saida, Tom::Secundaria, true, "Notes:"),
                );
                ui::linha(Fluxo::Saida, &e.notes);
            }
            ui::campo("Set on:", data(data_da_senha(e)));
            ui::linha(
                Fluxo::Saida,
                ui::apagado(format!(
                    "Created {} · changed {}",
                    data(e.created_at),
                    data(e.updated_at)
                )),
            );
            let antigas: Vec<_> = e
                .history
                .iter()
                .rev()
                .filter(|h| h.password != e.password)
                .collect();
            if historico {
                for h in &antigas {
                    ui::linha(
                        Fluxo::Saida,
                        format!(
                            "  {} {}",
                            ui::apagado(format!("old ({}):", data(h.changed_at))),
                            h.password
                        ),
                    );
                }
            } else if !antigas.is_empty() {
                ui::dica(
                    Fluxo::Saida,
                    format!("{} old password(s) (use --history).", antigas.len()),
                );
            }
            Ok(())
        }
        Comando::Copiar {
            entrada,
            usuario,
            segundos,
        } => {
            let s = Sessao::abrir(cfg, cli.sem_sync)?;
            let e = s.cofre.entrada(achar(&s.cofre, &entrada)?).map_err(erro)?;
            let texto = Zeroizing::new(if usuario {
                e.username.clone()
            } else {
                e.password.clone()
            });
            ui::dica(Fluxo::Erro, &e.title);
            drop(s); // solta a trava do cofre antes de esperar
            terminal::copiar(&texto, segundos)
        }
        Comando::Editar {
            entrada,
            titulo,
            usuario,
            url,
            notas,
            etiquetas,
            sem_etiquetas,
            senha,
            gerar,
            sem_simbolos,
        } => {
            let mut s = Sessao::abrir(cfg, cli.sem_sync)?;
            let id = achar(&s.cofre, &entrada)?;
            let nova = match (gerar, senha) {
                (Some(n), _) => Some(Zeroizing::new(
                    gerar_senha(n, classes(sem_simbolos)).map_err(|e| e.to_string())?,
                )),
                (None, true) => Some(terminal::segredo("New account password: ")?),
                _ => None,
            };
            s.cofre
                .editar(
                    id,
                    |e| {
                        if let Some(t) = titulo {
                            e.title = t;
                        }
                        if let Some(u) = usuario {
                            e.username = u;
                        }
                        if let Some(u) = url {
                            e.url = u;
                        }
                        if let Some(n) = notas {
                            e.notes = n;
                        }
                        if sem_etiquetas {
                            e.tags.clear();
                        } else if !etiquetas.is_empty() {
                            e.tags = etiquetas;
                        }
                        if let Some(p) = &nova {
                            e.password = p.to_string();
                        }
                    },
                    agora(),
                )
                .map_err(erro)?;
            s.salvar()?;
            ui::sucesso(Fluxo::Saida, format!("Edited: {}", id_curto(&id)));
            Ok(())
        }
        Comando::Remover { entrada, sim } => {
            let mut s = Sessao::abrir(cfg, cli.sem_sync)?;
            let id = achar(&s.cofre, &entrada)?;
            let t = s.cofre.entrada(id).map_err(erro)?.title.clone();
            if !sim && !terminal::confirmar(&format!("Remove \"{t}\"?"))? {
                println!("Nothing removed.");
                return Ok(());
            }
            s.cofre.remover(id, agora()).map_err(erro)?;
            s.salvar()?;
            ui::sucesso(Fluxo::Saida, format!("Removed: {t}"));
            Ok(())
        }
        Comando::SenhaMestra => {
            let mut s = Sessao::abrir(cfg, cli.sem_sync)?;
            let nova = terminal::senha_nova("New master password")?;
            let rec = s
                .cofre
                .trocar_senha_mestra(nova.as_bytes(), ParametrosKdf::PADRAO)
                .map_err(erro)?;
            s.senha = nova;
            s.salvar()?;
            ui::sucesso(
                Fluxo::Saida,
                "Master password changed. The old recovery key is no longer valid.",
            );
            mostrar_recuperacao(&rec)?;
            if s.sync {
                ui::dica(
                    Fluxo::Saida,
                    "On the other devices, the next sync will ask for the new password.",
                );
            }
            Ok(())
        }
        Comando::Recuperar => {
            let caminho = cfg.cofre();
            let texto = terminal::segredo("Recovery key: ")?;
            let mut c = Cofre::abrir(&caminho, cfg.device_id, &Credencial::Recuperacao(&texto))
                .map_err(erro)?;
            let nova = terminal::senha_nova("New master password")?;
            let rec = c
                .trocar_senha_mestra(nova.as_bytes(), ParametrosKdf::PADRAO)
                .map_err(erro)?;
            c.salvar().map_err(erro)?;
            ui::sucesso(Fluxo::Saida, "Master password reset.");
            mostrar_recuperacao(&rec)?;
            if cfg.pasta_sync.is_some() && !cli.sem_sync {
                let mut s = Sessao {
                    cfg,
                    cofre: c,
                    senha: nova,
                    sync: true,
                };
                s.sincronizar(false)?;
            }
            Ok(())
        }
        Comando::Importar { arquivo } => {
            let bytes = Zeroizing::new(
                std::fs::read(&arquivo)
                    .map_err(|_| format!("could not read {}", arquivo.display()))?,
            );
            let kdbx = bytes.starts_with(&[0x03, 0xd9, 0xa2, 0x9a]);
            let registros = if kdbx {
                let sk = terminal::segredo("KeePass file password: ")?;
                ler_kdbx(&bytes, &sk).map_err(|e| e.to_string())?
            } else {
                ler_csv(&bytes).map_err(|e| e.to_string())?
            };
            let mut s = Sessao::abrir(cfg, cli.sem_sync)?;
            let r = s.cofre.importar(&registros, agora()).map_err(erro)?;
            s.salvar()?;
            ui::sucesso(
                Fluxo::Saida,
                format!(
                    "Imported: {} · already existed: {} · rejected (field too large): {}",
                    r.adicionadas, r.duplicadas, r.invalidas
                ),
            );
            if !kdbx {
                ui::aviso("remember to delete the source CSV: it is in plain text.");
            }
            Ok(())
        }
        Comando::Exportar {
            arquivo,
            em_claro,
            forcar,
        } => {
            if arquivo.exists() && !forcar {
                return Err(format!(
                    "{} already exists (use --force)",
                    arquivo.display()
                ));
            }
            let ext = arquivo
                .extension()
                .and_then(|e| e.to_str())
                .map(str::to_lowercase)
                .unwrap_or_default();
            if ext != "kdbx" && ext != "csv" {
                return Err("use the .kdbx or .csv extension".into());
            }
            if ext == "csv" && !em_claro {
                return Err(
                    "a CSV is unencrypted on disk. Confirm with --plaintext or prefer .kdbx."
                        .into(),
                );
            }
            let s = Sessao::abrir(cfg, cli.sem_sync)?;
            let regs = s.cofre.registros();
            let bytes = if ext == "kdbx" {
                let sk = terminal::senha_nova("KeePass file password")?;
                ui::dica(Fluxo::Erro, "Generating the KDBX...");
                gerar_kdbx(&regs, &sk).map_err(|e| e.to_string())?
            } else {
                gerar_csv(&regs).map_err(|e| e.to_string())?
            };
            let bytes = Zeroizing::new(bytes);
            std::fs::write(&arquivo, bytes.as_slice())
                .map_err(|_| format!("could not write {}", arquivo.display()))?;
            ui::sucesso(
                Fluxo::Saida,
                format!(
                    "Exported {} entry(ies) to {}.",
                    regs.len(),
                    arquivo.display()
                ),
            );
            Ok(())
        }
    }
}

fn main() -> ExitCode {
    match executar(Cli::parse()) {
        Ok(()) => ExitCode::SUCCESS,
        Err(m) => {
            ui::falha(m);
            ExitCode::FAILURE
        }
    }
}
