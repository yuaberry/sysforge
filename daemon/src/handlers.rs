//! Despacho de métodos v1 — TODOS read-only, todos alimentados por sondagem
//! REAL via yua-core. Nenhum método retorna dado inventado.

use serde_json::json;

use yua_core::executor::Executor;
use yua_core::hw::system::probe_system_info;
use yua_core::capability::probe_capabilities;
use yua_core::disk::lsblk::list_blockdevices;
use yua_core::disk::udev::enrich_from_udev;
use yua_core::boot::efi::read_efi_state;
use yua_core::error::{ErrorDomain, YuaError};
use yua_core::ipc::protocol::{
    Request, Response, WireError, METHOD_CAPABILITIES, METHOD_DAEMON_INFO, METHOD_DISKS_LIST,
    METHOD_ECHO, METHOD_EFI_ENTRIES, METHOD_SYSTEM_INFO, PROTOCOL_VERSION,
};

use crate::{Config, DaemonMode, VERSION};

pub fn dispatch(req: &Request, cfg: &Config) -> Response {
    match handle(req, cfg) {
        Ok(value) => Response::ok(req.id, value),
        Err(e) => Response::err(req.id, WireError::from(e)),
    }
}

fn handle(req: &Request, cfg: &Config) -> Result<serde_json::Value, YuaError> {
    let exec = Executor::default();
    match req.method.as_str() {
        METHOD_ECHO => Ok(json!({ "echo": req.params.get("message").cloned().unwrap_or(json!("pong")) })),
        METHOD_SYSTEM_INFO => serde_json::to_value(probe_system_info()).map_err(YuaError::from),
        METHOD_DISKS_LIST => {
            let mut devs = list_blockdevices(&exec)?;
            for d in devs.iter_mut() {
                enrich_from_udev(d);
                for p in d.children.iter_mut() {
                    enrich_from_udev(p);
                }
            }
            serde_json::to_value(devs).map_err(YuaError::from)
        }
        METHOD_EFI_ENTRIES => serde_json::to_value(read_efi_state(&exec)?).map_err(YuaError::from),
        METHOD_CAPABILITIES => serde_json::to_value(probe_capabilities()).map_err(YuaError::from),
        METHOD_DAEMON_INFO => Ok(json!({
            "daemon": "yua-osd",
            "version": VERSION,
            "protocol": PROTOCOL_VERSION,
            "mode": cfg.mode.label(),
            "socket": cfg.socket.display().to_string(),
            "pid": std::process::id(),
            "uid_running": unsafe { libc::getuid() },
            "system_ready": cfg.mode == DaemonMode::System,
        })),
        other => {
            let e = YuaError::new(
                ErrorDomain::NotSupported,
                1,
                format!("Método desconhecido: {other}"),
            )
            .with_technical("protocolo v1 publica apenas: v1.echo, v1.system.info, v1.disks.list, v1.efi.entries, v1.capabilities, v1.daemon.info")
            .with_recommendation("Consulte docs/ARCHITECTURE.md — métodos de escrita chegam na Fase 2+.");
            Err(e)
        }
    }
}
