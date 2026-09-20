//! YUA OS MANAGER — app desktop (Tauri 2).
//!
//! Todos os comandos abaixo são READ-ONLY e rodam IN-PROCESS via yua-core
//! (sem daemon). Operações privilegiadas (escrita em disco/ESP/UEFI) chegarão
//! na Fase 2 e passarão EXCLUSIVAMENTE pelo daemon yua-osd — o app nunca
//! pedirá sudo direto.

use serde_json::Value;

use yua_core::boot::efi::read_efi_state;
use yua_core::boot::esp::read_esp;
use yua_core::capability::probe_capabilities;
use yua_core::disk::lsblk::list_blockdevices;
use yua_core::disk::smart::smart_health;
use yua_core::disk::udev::enrich_from_udev;
use yua_core::error::YuaError;
use yua_core::executor::Executor;
use yua_core::hw::system::{probe_system_info, read_secure_boot};

fn ok<T: serde::Serialize>(v: T) -> Value {
    serde_json::to_value(v).unwrap_or(Value::Null)
}

/// Erro como JSON estruturado — a UI sempre recebe código + motivo real.
fn err(e: YuaError) -> Value {
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
        "app": "yua-desktop",
        "version": yua_core::YUA_VERSION,
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

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .invoke_handler(tauri::generate_handler![
            get_app_info,
            get_system_info,
            get_secure_boot,
            get_esp,
            get_disks,
            get_smart,
            get_efi_state,
            get_capabilities
        ])
        .run(tauri::generate_context!())
        .expect("falha ao iniciar o YUA OS MANAGER");
}
