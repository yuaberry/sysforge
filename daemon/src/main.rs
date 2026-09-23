//! `sysforge-osd` — daemon do SYSFORGE.
//!
//! Dois modos:
//! - **dev** (padrão): socket em $XDG_RUNTIME_DIR, aceita apenas o próprio
//!   usuário (SO_PEERCRED), serve métodos read-only e recusa QUALQUER
//!   operação destrutiva com SF-AUTH-002. Fail-closed por construção.
//! - **system**: socket /run/sysforge-osd.sock (systemd socket activation),
//!   executa como root; métodos read-only liberados, destrutivos recusados
//!   até a Fase 2 (SF-AUTH-004). Toda chamada é auditada em JSONL.
//!
//! Este binário é a ÚNICA porta privilegiada da plataforma. O app desktop
//! e o CLI nunca pedem sudo direto — pedem ao daemon.

mod auth;
mod handlers;
mod server;

use std::path::{Path, PathBuf};

use sysforge_core::ipc::DEFAULT_SYSTEM_SOCKET;
use sysforge_core::logging;

pub const VERSION: &str = env!("CARGO_PKG_VERSION");

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum DaemonMode {
    Dev,
    System,
}

impl DaemonMode {
    pub fn label(self) -> &'static str {
        match self {
            DaemonMode::Dev => "dev",
            DaemonMode::System => "system",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Config {
    pub mode: DaemonMode,
    pub socket: PathBuf,
}

fn usage() -> ! {
    eprintln!("sysforge-osd {VERSION} — SYSFORGE OS Manager daemon");
    eprintln!();
    eprintln!("USO:");
    eprintln!("  sysforge-osd [--dev | --system] [--socket CAMINHO]");
    eprintln!();
    eprintln!("MODOS:");
    eprintln!("  --dev      modo desenvolvimento (padrão): somente leitura, mesmo usuário");
    eprintln!("  --system   modo sistema: requer root, socket /run/sysforge-osd.sock");
    eprintln!();
    eprintln!("OPÇÕES:");
    eprintln!("  --socket   caminho do socket unix (sobrepõe o padrão do modo)");
    eprintln!("  --version  versão");
    eprintln!("  --help     esta ajuda");
    std::process::exit(0)
}

fn parse_args() -> Config {
    let mut mode = DaemonMode::Dev;
    let mut socket: Option<PathBuf> = None;
    let mut args = std::env::args().skip(1);
    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--dev" => mode = DaemonMode::Dev,
            "--system" => mode = DaemonMode::System,
            "--socket" => match args.next() {
                Some(s) => socket = Some(PathBuf::from(s)),
                None => {
                    eprintln!("--socket requer um caminho");
                    std::process::exit(2);
                }
            },
            "--version" => {
                println!("sysforge-osd {VERSION}");
                std::process::exit(0);
            }
            "--help" | "-h" => usage(),
            other => {
                eprintln!("opção desconhecida: {other} (use --help)");
                std::process::exit(2);
            }
        }
    }
    let socket = socket.unwrap_or_else(|| match mode {
        DaemonMode::Dev => sysforge_core::ipc::dev_socket_default(),
        DaemonMode::System => PathBuf::from(DEFAULT_SYSTEM_SOCKET),
    });
    Config { mode, socket }
}

fn daemon_log_file() -> Option<PathBuf> {
    std::env::var_os("HOME").map(|h| {
        PathBuf::from(h).join(".local/share/sysforge/logs/daemon.log")
    })
}

fn main() {
    let cfg = parse_args();

    // Regras de modo:
    // - system exige root (acesso privilegiado real).
    // - dev nunca exige root: se alguém rodar com sudo, apenas avisa.
    match cfg.mode {
        DaemonMode::System => {
            if unsafe { libc::getuid() } != 0 {
                eprintln!("sysforge-osd: modo --system exige root (use o systemd service: install-daemon.sh)");
                std::process::exit(1);
            }
            logging::init(None, "info"); // stderr → journald
        }
        DaemonMode::Dev => {
            let _ = Path::new(&cfg.socket);
            logging::init(daemon_log_file().as_deref(), "info");
        }
    }

    tracing::info!(version = VERSION, mode = cfg.mode.label(), socket = %cfg.socket.display(), "sysforge-osd iniciando");

    if let Err(e) = server::serve(&cfg) {
        eprintln!("sysforge-osd: {e}");
        std::process::exit(1);
    }
}
