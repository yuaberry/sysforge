use std::path::PathBuf;

use yua_core::error::{ErrorDomain, YuaError};
use yua_core::executor::which;
use yua_core::ipc::client::YuaClient;
use yua_core::ipc::protocol::DEFAULT_SYSTEM_SOCKET;

use crate::DaemonAction;
use crate::ui::{self, paint};

pub fn run(
    json: bool,
    color: bool,
    action: Option<DaemonAction>,
    socket: Option<PathBuf>,
) -> Result<(), YuaError> {
    match action.unwrap_or(DaemonAction::Status) {
        DaemonAction::Status => status(json, color, socket),
        DaemonAction::Run => run_dev(socket),
        DaemonAction::System => start_system(color),
        DaemonAction::Stop => stop_system(color),
    }
}

fn status(json: bool, color: bool, socket: Option<PathBuf>) -> Result<(), YuaError> {
    let candidates: Vec<PathBuf> = match socket {
        Some(s) => vec![s],
        None => vec![
            yua_core::ipc::dev_socket_default(),
            PathBuf::from(DEFAULT_SYSTEM_SOCKET),
        ],
    };

    for path in &candidates {
        if let Ok(mut client) = YuaClient::connect(path) {
            let pong = client.call(yua_core::ipc::protocol::METHOD_ECHO, serde_json::json!({"message": "cli"}))?;
            let _ = pong;
            let info = client.call(yua_core::ipc::protocol::METHOD_DAEMON_INFO, serde_json::json!({}))?;
            if json {
                println!(
                    "{}",
                    serde_json::to_string_pretty(&serde_json::json!({"ok": true, "daemon": info, "socket": path.display().to_string()}))?
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
                "  {} leitura: v1.echo · system.info · disks.list · efi.entries · capabilities · boot.snapshot",
                ui::tag_info(color)
            );
            println!(
                "  {} privilegiado (só system+polkit): boot.set_next/clear_next/remove_entry · reboot_to_firmware/arm_firmware · system.reboot/poweroff",
                ui::tag_info(color)
            );
            println!(
                "  {} destrutivo futuro: RECUSADO ({})",
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
        candidates.iter().map(|p| p.display().to_string()).collect::<Vec<_>>().join(", ")
    ))
    .with_recommendation(
        "`yua daemon system` sobe com privilégio via polkit (sua senha). `yua daemon run` = modo dev somente leitura. Sistema completo: bash scripts/install-daemon.sh.",
    ))
}

/// Garante um daemon em modo SYSTEM rodando: conecta se já existir; senão
/// sobe via pkexec — o polkit abre o diálogo de senha NA TELA do usuário.
pub fn ensure_system_daemon(color: bool) -> Result<PathBuf, YuaError> {
    println!(
        "{} iniciando daemon em modo SISTEMA via pkexec…",
        ui::tag_info(color)
    );
    println!(
        "  {} AUTORIZE NO DIÁLOGO QUE VAI APARECER NA SUA TELA (senha do seu usuário)",
        ui::tag_warn(color)
    );
    let sock = yua_core::ipc::system::ensure_system_daemon(
        std::time::Duration::from_secs(90),
        |s| {
            use std::io::Write;
            print!("\r  aguardando autorização… {s}s ");
            std::io::stdout().flush().ok();
        },
    )?;
    println!();
    println!("{} daemon system pronto — privilégios liberados via polkit", ui::tag_ok(color));
    Ok(sock)
}

fn start_system(color: bool) -> Result<(), YuaError> {
    let sock = ensure_system_daemon(color)?;
    let mut client = YuaClient::connect(&sock)?;
    let info = client.call(yua_core::ipc::protocol::METHOD_DAEMON_INFO, serde_json::json!({}))?;
    println!(
        "  {} modo {} · pid {} · {}",
        ui::tag_ok(color),
        paint(info["mode"].as_str().unwrap_or("?"), "green", color),
        info["pid"],
        sock.display()
    );
    Ok(())
}

fn stop_system(color: bool) -> Result<(), YuaError> {
    let sock = ensure_system_daemon(color)?;
    let mut client = YuaClient::connect(&sock)?;
    let r = client.call(
        yua_core::ipc::protocol::METHOD_DAEMON_SHUTDOWN,
        serde_json::json!({ "confirm": true }),
    )?;
    println!(
        "  {} daemon encerrando ({})",
        ui::tag_ok(color),
        r["shutting_down"]
    );
    Ok(())
}

fn run_dev(socket: Option<PathBuf>) -> Result<(), YuaError> {
    let exe = std::env::current_exe()?;
    let osd = exe
        .parent()
        .map(|d| d.join("yua-osd"))
        .filter(|p| p.is_file())
        .or_else(|| which("yua-osd"))
        .ok_or_else(|| {
            YuaError::new(ErrorDomain::Dep, 3, "Binário yua-osd não encontrado")
                .with_recommendation("Rode `cargo build --workspace` (fica em target/debug/) ou build --release.")
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
