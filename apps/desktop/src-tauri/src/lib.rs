//! SYSFORGE — app desktop (Tauri 2).
//!
//! Todos os comandos abaixo são READ-ONLY e rodam IN-PROCESS via sysforge-core
//! (sem daemon). Operações privilegiadas (escrita em disco/ESP/UEFI) chegarão
//! na Fase 2 e passarão EXCLUSIVAMENTE pelo daemon sysforge-osd — o app nunca
//! pedirá sudo direto.

use serde_json::Value;

use sysforge_core::boot::efi::read_efi_state;
use sysforge_core::boot::esp::read_esp;
use sysforge_core::capability::probe_capabilities;
use sysforge_core::disk::lsblk::list_blockdevices;
use sysforge_core::disk::smart::smart_health;
use sysforge_core::disk::udev::enrich_from_udev;
use sysforge_core::error::SysforgeError;
use sysforge_core::executor::Executor;
use sysforge_core::hw::system::{probe_system_info, read_secure_boot};
use sysforge_core::ipc::client::YuaClient;
use sysforge_core::ipc::protocol::DEFAULT_SYSTEM_SOCKET;
use sysforge_core::windows::checklist::run_checklist;
use sysforge_core::windows::media::list_removable_media;
use sysforge_core::windows::unattend::{generate_autounattend, UnattendConfig};

fn ok<T: serde::Serialize>(v: T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// Erro como JSON estruturado — a UI sempre recebe código + motivo real.
fn err(e: SysforgeError) -> Value {
    serde_json::json!({
        "error": {
            "code": e.code,
            "message": e.message,
            "technical": e.technical,
            "recommendation": e.recommendation,
        }
    })
}

#[tauri::command]
fn get_app_info() -> Value {
    ok(serde_json::json!({
        "app": "sysforge-desktop",
        "version": sysforge_core::YUA_VERSION,
        "backend": "in-process (somente leitura)",
        "daemon_required_for": "operações privilegiadas (Fase 2+)",
    }))
}

#[tauri::command]
fn get_system_info() -> Value {
    ok(probe_system_info())
}

#[tauri::command]
fn get_secure_boot() -> Value {
    ok(read_secure_boot())
}

#[tauri::command]
fn get_esp() -> Value {
    ok(read_esp())
}

#[tauri::command]
fn get_disks() -> Value {
    let exec = Executor::default();
    match list_blockdevices(&exec) {
        Ok(mut devs) => {
            for d in devs.iter_mut() {
                enrich_from_udev(d);
                for p in d.children.iter_mut() {
                    enrich_from_udev(p);
                }
            }
            ok(devs)
        }
        Err(e) => err(e),
    }
}

#[tauri::command]
fn get_smart(disk: String) -> Value {
    ok(smart_health(&Executor::default(), &disk))
}

#[tauri::command]
fn get_efi_state() -> Value {
    match read_efi_state(&Executor::default()) {
        Ok(state) => ok(state),
        Err(e) => err(e),
    }
}

#[tauri::command]
fn get_capabilities() -> Value {
    ok(probe_capabilities())
}

// ---------- proxy ao daemon (toda ação privilegiada passa por ele) ----------

/// Chama um método no daemon system. Erros vêm como JSON estruturado
/// (WireError) — a UI mostra código + mensagem + recomendação.
#[tauri::command]
fn daemon_call(method: String, params: Value) -> Result<Value, Value> {
    let sock = std::path::Path::new(DEFAULT_SYSTEM_SOCKET);
    let mut client = YuaClient::connect(sock).map_err(wire_err)?;
    client.call_interactive(&method, params).map_err(wire_err)
}

/// Sobe o daemon system via pkexec (polkit pede a senha NA TELA).
#[tauri::command]
fn ensure_system_daemon() -> Result<Value, Value> {
    sysforge_core::ipc::system::ensure_system_daemon(std::time::Duration::from_secs(90), |_| {})
        .map(|sock| serde_json::json!({ "socket": sock.display().to_string() }))
        .map_err(wire_err)
}

fn wire_err(e: SysforgeError) -> Value {
    serde_json::json!({
        "code": e.code,
        "message": e.message,
        "technical": e.technical,
        "recommendation": e.recommendation,
    })
}

// ---------- fluxo Windows 11 ----------

#[tauri::command]
fn windows_checklist() -> Result<Value, Value> {
    let ex = Executor::default();
    run_checklist(&ex).map(|c| serde_json::to_value(c).unwrap_or(Value::Null)).map_err(wire_err)
}

#[tauri::command]
fn unattend_generate(edition: String, full_wipe: bool) -> Result<Value, Value> {
    let cfg = UnattendConfig {
        edition,
        full_disk_wipe: full_wipe,
        ..UnattendConfig::default()
    };
    generate_autounattend(&cfg)
        .map(|xml| serde_json::json!({ "xml": xml }))
        .map_err(wire_err)
}

/// Gera E grava o autounattend no destino certo (raiz do Ventoy montado;
/// senão ~/Downloads) — o Rust conhece o HOME, o browser não.
#[tauri::command]
fn unattend_save(edition: String, full_wipe: bool) -> Result<Value, Value> {
    let cfg = UnattendConfig {
        edition,
        full_disk_wipe: full_wipe,
        ..UnattendConfig::default()
    };
    let xml = generate_autounattend(&cfg).map_err(wire_err)?;
    let ex = Executor::default();
    let media = list_removable_media(&ex).map_err(wire_err)?;
    let ventoy_mount = media
        .iter()
        .find(|m| m.is_ventoy)
        .and_then(|m| m.mounted_at.clone());
    let path = match (&ventoy_mount, std::env::var_os("HOME")) {
        (Some(m), _) => std::path::Path::new(m).join("autounattend.xml"),
        (None, Some(h)) => std::path::Path::new(&h).join("Downloads").join("autounattend.xml"),
        (None, None) => std::path::PathBuf::from("/tmp/autounattend.xml"),
    };
    std::fs::write(&path, &xml).map_err(|e| {
        wire_err(
            SysforgeError::new(
                sysforge_core::error::ErrorDomain::Io,
                13,
                format!("Falha ao gravar {}", path.display()),
            )
            .with_technical(e.to_string()),
        )
    })?;
    Ok(serde_json::json!({
        "path": path.display().to_string(),
        "on_ventoy": ventoy_mount.is_some(),
        "full_wipe": cfg.full_disk_wipe,
    }))
}

#[tauri::command]
fn list_media() -> Result<Value, Value> {
    let ex = Executor::default();
    list_removable_media(&ex)
        .map(|m| serde_json::to_value(m).unwrap_or(Value::Null))
        .map_err(wire_err)
}

/// Grava um arquivo texto (autounattend.xml) no caminho escolhido.
#[tauri::command]
fn save_text_file(path: String, contents: String) -> Result<Value, Value> {
    std::fs::write(&path, contents)
        .map(|_| serde_json::json!({ "saved": path }))
        .map_err(|e| {
            wire_err(
                SysforgeError::new(sysforge_core::error::ErrorDomain::Io, 13, format!("Falha ao gravar {path}"))
                    .with_technical(e.to_string()),
            )
        })
}

/// Verifica se já existe um agente de diálogo polkit na sessão
/// (GNOME/MATE/Cinnamon normalmente registram um no login).
fn polkit_agent_running() -> bool {
    std::fs::read_dir("/proc").ok().map(|rd| {
        rd.filter_map(|e| e.ok())
            .filter(|e| e.file_name().to_string_lossy().chars().all(|c| c.is_ascii_digit()))
            .any(|e| {
                let cmdline = std::fs::read_to_string(format!("/proc/{}/cmdline", e.file_name().to_string_lossy()))
                    .unwrap_or_default();
                cmdline.contains("authentication-agent")
            })
    }).unwrap_or(false)
}

#[tauri::command]
fn get_network_info() -> Result<Value, Value> {
    use sysforge_core::net;
    let exec = sysforge_core::executor::Executor::default();
    net::info(&exec).map_err(wire_err).map(|i| serde_json::to_value(i).unwrap_or(Value::Null))
}

#[tauri::command]
fn backup_state() -> Result<Value, Value> {
    let dirs = sysforge_core::backup::user_dirs();
    let targets = sysforge_core::backup::targets();
    Ok(serde_json::json!({ "dirs": dirs, "targets": targets }))
}

#[tauri::command]
fn backup_estimate(dirs: Vec<String>, target: String) -> Result<Value, Value> {
    sysforge_core::backup::estimate(&dirs, &target).map_err(wire_err).and_then(|e| Ok(serde_json::to_value(e).unwrap_or(Value::Null)))
}

#[tauri::command]
fn backup_run(dirs: Vec<String>, target: String) -> Result<Value, Value> {
    let exec = sysforge_core::executor::Executor::default();
    sysforge_core::backup::run(&exec, &dirs, &target).map_err(wire_err)
}

/// Garante que exista um agente de diálogo polkit para a sessão do app.
/// Sem ele, pedidos de autorização do daemon morrem em silêncio
/// (pkcheck fica esperando um diálogo que ninguém mostra — bug real
/// encontrado em campo). Se a sessão já tem agente, não faz nada.
/// O .deb depende de mate-polkit — o binário existe em Ubuntu/Mint.
fn ensure_polkit_agent() {
    if polkit_agent_running() {
        return;
    }
    let candidates = [
        "/usr/libexec/polkit-mate-authentication-agent-1",
        "/usr/lib/x86_64-linux-gnu/polkit-mate-authentication-agent-1",
        "/usr/lib/polkit-mate/polkit-mate-authentication-agent-1",
        "/usr/lib/policykit-1-gnome/polkit-gnome-authentication-agent-1",
    ];
    for bin in candidates {
        if !std::path::Path::new(bin).exists() {
            continue;
        }
        match std::process::Command::new(bin)
            .stdout(std::process::Stdio::null())
            .stderr(std::process::Stdio::null())
            .spawn()
        {
            // "already exists" sai rápido e silencioso — tolerável.
            Ok(_) => { eprintln!("sysforge: agente de diálogo polkit iniciado ({bin})"); return; }
            Err(e) => eprintln!("sysforge: não pôde iniciar o agente polkit {bin}: {e}"),
        }
    }
    eprintln!("sysforge: nenhum agente de diálogo polkit disponível — autorizações via app podem não mostrar senha");
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    ensure_polkit_agent();
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            get_system_info,
            get_secure_boot,
            get_esp,
            get_disks,
            get_smart,
            get_efi_state,
            get_capabilities,
            daemon_call,
            ensure_system_daemon,
            windows_checklist,
            unattend_generate,
            unattend_save,
            list_media,
            save_text_file,
            get_network_info,
            backup_state,
            backup_estimate,
            backup_run
        ])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o SYSFORGE");
}
