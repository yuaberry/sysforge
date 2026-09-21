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
    /// Estado UEFI + controle de boot: BootNext, reboot na BIOS, limpeza
    Boot {
        #[command(subcommand)]
        action: Option<BootAction>,
    },
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
    /// Energia da máquina: reiniciar / desligar (exige --confirm)
    Power {
        #[command(subcommand)]
        action: PowerAction,
        /// Confirma ação imediata
        #[arg(long)]
        confirm: bool,
    },
    /// Fluxo real de instalação do Windows 11 (checklist → USB → BootNext)
    Winstall {
        /// Executa as ações (sem isto: apenas diagnóstico)
        #[arg(long)]
        apply: bool,
        /// ISO a copiar (padrão: auto-detecta Win11 em ~/Downloads)
        #[arg(long)]
        iso: Option<String>,
        /// Edição do Windows 11: pro (padrão) ou home
        #[arg(long, default_value = "pro")]
        edition: String,
        /// autounattend apaga o disco 0 na instalação (PEDIRÁ confirmação digitada)
        #[arg(long)]
        full_wipe: bool,
        /// Reinicia após aplicar (boot pelo pendrive)
        #[arg(long)]
        reboot: bool,
    },
    /// Mostra as últimas linhas do log do YUA
    Logs {
        /// Quantidade de linhas a exibir
        #[arg(long, default_value_t = 30)]
        lines: usize,
    },
}

#[derive(Subcommand, Clone)]
enum BootAction {
    /// Define BootNext one-shot (auto-detecta USB se ID omitido)
    Next {
        /// ID da entrada UEFI (ex.: 0014)
        entry: Option<String>,
        /// Cancela um BootNext armado
        #[arg(long)]
        clear: bool,
    },
    /// Reinicia o computador direto na tela do BIOS/UEFI
    Firmware {
        /// Só arma: próximo boot vai à BIOS, sem reiniciar agora
        #[arg(long)]
        arm: bool,
    },
    /// Remove entrada de boot (snapshot antes; internas do firmware são intocáveis)
    Remove {
        entry: String,
    },
}

#[derive(Subcommand, Clone)]
enum PowerAction {
    /// Reinicia a máquina agora
    Reboot,
    /// Desliga a máquina agora
    Off,
}

#[derive(Subcommand, Clone)]
enum DaemonAction {
    /// Consulta estado do daemon e do protocolo IPC
    Status,
    /// Executa o daemon em primeiro plano (modo dev, somente leitura)
    Run,
    /// Sobe o daemon em modo SISTEMA via pkexec (polkit pede sua senha)
    System,
    /// Encerra o daemon system de forma limpa (polkit pede sua senha)
    Stop,
}

fn main() {
    let cli = Cli::parse();

    let color = !cli.no_color
        && std::io::stdout().is_terminal()
        && std::io::stderr().is_terminal()
        && std::env::var_os("NO_COLOR").is_none();
    colored::control::set_override(color);

    logging::init(logging::default_app_log_file().as_deref(), "info");

    let json = cli.json;
    let result: Result<(), YuaError> = match cli.command {
        Commands::Status => commands::status::run(json, color),
        Commands::Disks => commands::disks::run(json, color),
        Commands::Boot { action } => commands::boot::run(json, color, action),
        Commands::Doctor => commands::doctor::run(json, color),
        Commands::Daemon { action, socket } => {
            commands::daemon::run(json, color, action, socket)
        }
        Commands::Power { action, confirm } => commands::power::run(json, color, action, confirm),
        Commands::Winstall {
            apply,
            iso,
            edition,
            full_wipe,
            reboot,
        } => commands::winstall::run(
            json,
            color,
            commands::winstall::WinstallOpts {
                apply,
                iso,
                edition,
                full_wipe,
                reboot,
            },
        ),
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
