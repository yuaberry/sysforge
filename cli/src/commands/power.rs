//! `sysforge power reboot|off` — energia via daemon system (polkit autoriza na tela).

use std::path::PathBuf;

use serde_json::json;

use sysforge_core::error::SysforgeError;
use sysforge_core::ipc::client::YuaClient;
use sysforge_core::ipc::protocol::{METHOD_SYSTEM_POWEROFF, METHOD_SYSTEM_REBOOT};

use crate::commands::daemon::ensure_system_daemon;
use crate::ui::{self, paint};

use crate::PowerAction;

pub fn run(json: bool, color: bool, action: PowerAction, confirm: bool) -> Result<(), SysforgeError> {
    let (method, label) = match action {
        PowerAction::Reboot => (METHOD_SYSTEM_REBOOT, "reiniciar"),
        PowerAction::Off => (METHOD_SYSTEM_POWEROFF, "desligar"),
    };

    if !confirm {
        let e = SysforgeError::new(
            sysforge_core::error::ErrorDomain::Auth,
            6,
            format!("Use --confirm para {label} agora"),
        )
        .with_recommendation(format!("`sysforge power {} --confirm` — ação imediata e sem prompt.", match action { PowerAction::Reboot => "reboot", PowerAction::Off => "off" }));
        if json {
            println!("{}", serde_json::json!({"ok": false, "error": {"code": e.code, "message": e.message}}));
        }
        return Err(e);
    }

    let sock: PathBuf = ensure_system_daemon(color)?;
    let mut client = YuaClient::connect(&sock)?;

    println!("  {} {} em 3s — Ctrl+C para abortar…", ui::tag_warn(color), label);
    std::thread::sleep(std::time::Duration::from_secs(1));
    println!("  {} 2s…", paint("·", "dim", color));
    std::thread::sleep(std::time::Duration::from_secs(1));
    println!("  {} 1s…", paint("·", "dim", color));
    std::thread::sleep(std::time::Duration::from_secs(1));

    let result = client.call_interactive(method, json!({ "confirm": true }))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&result)?);
    } else {
        println!("  {} {}", ui::tag_ok(color), match action {
            PowerAction::Reboot => "reiniciando…".to_string(),
            PowerAction::Off => "desligando…".to_string(),
        });
    }
    Ok(())
}
