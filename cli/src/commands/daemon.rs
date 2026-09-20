//! `yua daemon` — consulta e executa o daemon yua-osd (IPC privilegiado).

use std::path::PathBuf;

use serde_json::json;

use yua_core::error::{ErrorDomain, YuaError};
use yua_core::executor::which;
use yua_core::ipc::protocol::{DEFAULT_SYSTEM_SOCKET, METHOD_DAEMON_INFO, METHOD_ECHO};
use yua_core::ipc::{dev_socket_default, YuaClient};

use crate::ui::{self, paint};
use crate::DaemonAction;

pub fn run(
    json: bool,
    color: bool,
    action: Option<DaemonAction>,
    socket: Option<PathBuf>,
) -> Result<(), YuaError> {
    match action.unwrap_or(DaemonAction::Status) {
        DaemonAction::Status => status(json, color, socket),
        DaemonAction::Run => run_dev(socket),
    }
}

fn status(json: bool, color: bool, socket: Option<PathBuf>) -> Result<(), YuaError> {
    let candidates: Vec<PathBuf> = match socket {
        Some(s) => vec![s],
        None => vec![dev_socket_default(), PathBuf::from(DEFAULT_SYSTEM_SOCKET)],
    };

    for path in &candidates {
        if let Ok(mut client) = YuaClient::connect(path) {
            // eco de sanidade + info do daemon
            let pong = client.call(METHOD_ECHO, json!({"message": "cli"}))?;
            let _ = pong;
            let info = client.call(METHOD_DAEMON_INFO, json!({}))?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&json!({"ok": true, "daemon": info, "socket": path.display().to_string()}))?
                );
                return Ok(());
            }
            ui::banner(color);
            println!("  {} daemon yua-osd ativo", ui::tag_ok(color));
            let kv = |k: &str, v: String| {
                println!("  {} {}", paint(&format!("{:<12}", k), "dim", color), v);
            };
            kv("versão", info["version"].as_str().unwrap_or("?").to_string());
            kv(
                "modo",
                match info["mode"].as_str() {
                    Some("system") => paint("system (root, completo)", "green", color),
                    Some("dev") => paint("dev (somente leitura, fail-closed)", "cyan", color),
                    _ => "?".into(),
                },
            );
            kv("pid", info["pid"].to_string());
            kv("socket", path.display().to_string());
            println!();
            println!(
                "  {} métodos v1: v1.echo · v1.system.info · v1.disks.list · v1.efi.entries · v1.capabilities · v1.daemon.info",
                ui::tag_info(color)
            );
            println!(
                "  {} métodos destrutivos: RECUSADOS neste estágio ({})",
                ui::tag_fail(color),
                paint("YUA-AUTH-002/004 — fail-closed", "red", color)
            );
            return Ok(());
        }
    }

    Err(YuaError::new(
        ErrorDomain::Io,
        5,
        "Nenhum daemon yua-osd está respondendo",
    )
    .with_technical(format!(
        "sockets tentados: {}",
        candidates
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    ))
    .with_recommendation(
        "Para leitura, o CLI funciona sem daemon. Para subir o modo dev: `yua daemon run`. Para modo system (root): bash scripts/install-daemon.sh (systemd + polkit).",
    ))
}

fn run_dev(socket: Option<PathBuf>) -> Result<(), YuaError> {
    // Localiza o binário irmão (mesmo dir do executável yua) ou no PATH.
    let exe = std::env::current_exe()?;
    let sibling = exe
        .parent()
        .map(|d| d.join("yua-osd"))
        .filter(|p| p.is_file());
    let osd = sibling
        .or_else(|| which("yua-osd"))
        .ok_or_else(|| {
            YuaError::new(
                ErrorDomain::Dep,
                3,
                "Binário yua-osd não encontrado ao lado do `yua` nem no PATH",
            )
            .with_recommendation("Rode `cargo build --workspace` (o binário fica em target/debug/) ou instale o pacote completo.")
        })?;

    println!("executando daemon dev: {} --dev", osd.display());
    if let Some(s) = socket.as_ref() {
        println!("  socket: {}", s.display());
    }

    let mut cmd = std::process::Command::new(osd);
    cmd.arg("--dev");
    if let Some(s) = socket.as_ref() {
        cmd.arg("--socket").arg(s);
    }
    let status = cmd.status()?;
    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}
