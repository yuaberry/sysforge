//! `yua` — CLI de terminal do YUA OS MANAGER.
//!
//! Executável SEPARADO do app desktop (requisito do projeto), com efeitos
//! visuais: banner em gradiente ANSI, spinner braille, tabelas box-drawing,
//! barras e badges coloridos — todos respeitando NO_COLOR/pipe/`--json`.

mod commands;
mod ui;

use std::io::IsTerminal;

use clap::{Parser, Subcommand};

use yua_core::error::YuaError;
use yua_core::logging;

#[derive(Parser)]
#[command(
    name = "yua",
    version,
    about = "YUA OS MANAGER — deployment & recovery universal · CLI de terminal",
    disable_help_subcommand = true
)]
struct Cli {
    /// Saída JSON estruturada (para scripts e pipes)
    #[arg(long, global = true)]
    json: bool,
    /// Desabilita cores e efeitos visuais
    #[arg(long, global = true)]
    no_color: bool,
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Visão geral do sistema vivo: SO, kernel, memória, boot, energia
    Status,
    /// Inventário de blocos: discos, partições, filesystems, saúde
    Disks,
    /// Estado UEFI real: entradas de boot, BootOrder, Secure Boot, ESP
    Boot,
    /// Diagnóstico completo do ambiente com instruções exatas de correção
    Doctor,
    /// Controle do daemon yua-osd (IPC privilegiado)
    Daemon {
        #[command(subcommand)]
        action: Option<DaemonAction>,
        /// Socket unix alternativo do daemon
        #[arg(long)]
        socket: Option<std::path::PathBuf>,
    },
    /// Mostra as últimas linhas do log do YUA
    Logs {
        /// Quantidade de linhas a exibir
        #[arg(long, default_value_t = 30)]
        lines: usize,
    },
}

#[derive(Subcommand, Clone)]
enum DaemonAction {
    /// Consulta estado do daemon e do protocolo IPC
    Status,
    /// Executa o daemon em primeiro plano (modo dev, somente leitura)
    Run,
}

fn main() {
    let cli = Cli::parse();

    let color = !cli.no_color
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal()
        && std::env::var_os("NO_COLOR").is_none();
    colored::control::set_override(color);

    // Log sempre ligado (em arquivo), a partir do primeiro comando real.
    logging::init(logging::default_app_log_file().as_deref(), "info");

    let json = cli.json;
    let result: Result<(), YuaError> = match cli.command {
        Commands::Status => commands::status::run(json, color),
        Commands::Disks => commands::disks::run(json, color),
        Commands::Boot => commands::boot::run(json, color),
        Commands::Doctor => commands::doctor::run(json, color),
        Commands::Daemon { action, socket } => {
            commands::daemon::run(json, color, action, socket)
        }
        Commands::Logs { lines } => commands::logs::run(json, color, lines),
    };

    if let Err(e) = result {
        if json {
            println!(
                "{}",
                serde_json::json!({
                    "ok": false,
                    "error": {
                        "code": e.code,
                        "message": e.message,
                        "technical": e.technical,
                        "recommendation": e.recommendation,
                    }
                })
            );
        } else {
            eprintln!("{e}");
        }
        std::process::exit(1);
    }
}
