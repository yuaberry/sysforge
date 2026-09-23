//! Protocolo IPC do daemon `sysforge-osd`: NDJSON (uma mensagem JSON por linha)
//! sobre Unix socket. Versão v1, métodos read-only.
//!
//! Regras de segurança:
//! - O daemon SEMPRE conhece o uid do chamador via SO_PEERCRED (sem confiar
//!   em nada que o cliente envie).
//! - Modo dev: mesmo uid ⇒ read-only OK; destrutivo recusado (fail-closed).
//! - Modo sistema: autorização por método via polkit (pkcheck).
//! - Erros carregam código SF-XXX-NNN — nada de "erro genérico".

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::SysforgeError;

pub const PROTOCOL_VERSION: u32 = 1;

// ---- Métodos v1 (read-only) ----
pub const METHOD_ECHO: &str = "v1.echo";
pub const METHOD_SYSTEM_INFO: &str = "v1.system.info";
pub const METHOD_DISKS_LIST: &str = "v1.disks.list";
pub const METHOD_EFI_ENTRIES: &str = "v1.efi.entries";
pub const METHOD_CAPABILITIES: &str = "v1.capabilities";
pub const METHOD_DAEMON_INFO: &str = "v1.daemon.info";

// ---- Métodos privilegiados v2 (implementados, exigem daemon system + polkit) ----
/// BootNext one-shot (nunca toca BootOrder). Params: {entry_id, confirm:true}
pub const METHOD_BOOT_SET_NEXT: &str = "v1.boot.set_next";
/// Limpa um BootNext armado. Params: {confirm:true}
pub const METHOD_BOOT_CLEAR_NEXT: &str = "v1.boot.clear_next";
/// Remove entrada de boot (apenas entradas de disco/arquivo, com snapshot).
/// Params: {entry_id, confirm:true}
pub const METHOD_BOOT_REMOVE_ENTRY: &str = "v1.boot.remove_entry";
/// Snapshot do estado UEFI (read-only, funciona em dev).
pub const METHOD_BOOT_SNAPSHOT: &str = "v1.boot.snapshot";
/// Reinicia direto na tela de setup do firmware (BIOS). Params: {confirm:true}
pub const METHOD_BOOT_REBOOT_TO_FIRMWARE: &str = "v1.boot.reboot_to_firmware";
/// Só arma OsIndications (próximo boot vai pra BIOS, sem reiniciar agora).
/// Params: {confirm:true}
pub const METHOD_BOOT_ARM_FIRMWARE: &str = "v1.boot.arm_firmware";
/// Reboot imediato. Params: {confirm:true}
pub const METHOD_SYSTEM_REBOOT: &str = "v1.system.reboot";
/// Desligamento imediato. Params: {confirm:true}
pub const METHOD_SYSTEM_POWEROFF: &str = "v1.system.poweroff";
/// Encerra o daemon de forma limpa (para trocar binário sem sudo/reboot).
/// Params: {confirm:true}
pub const METHOD_DAEMON_SHUTDOWN: &str = "v1.daemon.shutdown";

/// Métodos que exigem daemon em modo SISTEMA + autorização polkit
/// (action com.sysforge.osd.lowrisk). Em modo dev são recusados (fail-closed).
pub const PRIVILEGED_METHODS: &[&str] = &[
    METHOD_BOOT_SET_NEXT,
    METHOD_BOOT_CLEAR_NEXT,
    METHOD_BOOT_REMOVE_ENTRY,
    METHOD_BOOT_REBOOT_TO_FIRMWARE,
    METHOD_BOOT_ARM_FIRMWARE,
    METHOD_SYSTEM_REBOOT,
    METHOD_SYSTEM_POWEROFF,
    METHOD_DAEMON_SHUTDOWN,
];

/// Métodos que serão destrutivos no futuro (wipe/format/deploy) — recusados
/// em TODOS os modos até a Fase que os implementar com guard completo.
pub const DESTRUCTIVE_REGISTRY: &[&str] = &[
    "v1.disk.wipe",
    "v1.disk.format",
    "v1.deploy.start",
];

/// Socket do daemon em modo sistema (systemd socket activation).
pub const DEFAULT_SYSTEM_SOCKET: &str = "/run/sysforge-osd.sock";

/// Socket dev padrão: $XDG_RUNTIME_DIR/sysforge-osd.dev.sock (ou /tmp).
pub fn dev_socket_default() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR") {
        PathBuf::from(xdg).join("sysforge-osd.dev.sock")
    } else {
        PathBuf::from("/tmp").join(format!("sysforge-osd.dev.{}.sock", std::env::var("UID").unwrap_or_else(|_| "0".into())))
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Request {
    pub id: u64,
    pub method: String,
    #[serde(default)]
    pub params: serde_json::Value,
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct WireError {
    pub code: String,
    pub message: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub technical: String,
    #[serde(default, skip_serializing_if = "String::is_empty")]
    pub recommendation: String,
}

impl From<SysforgeError> for WireError {
    fn from(e: SysforgeError) -> Self {
        Self {
            code: e.code,
            message: e.message,
            technical: e.technical,
            recommendation: e.recommendation,
        }
    }
}

impl From<WireError> for SysforgeError {
    fn from(e: WireError) -> Self {
        Self {
            code: e.code,
            message: e.message,
            technical: e.technical,
            recommendation: e.recommendation,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Response {
    pub id: u64,
    pub ok: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result: Option<serde_json::Value>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<WireError>,
}

impl Response {
    pub fn ok(id: u64, result: serde_json::Value) -> Self {
        Self {
            id,
            ok: true,
            result: Some(result),
            error: None,
        }
    }
    pub fn err(id: u64, e: impl Into<WireError>) -> Self {
        Self {
            id,
            ok: false,
            result: None,
            error: Some(e.into()),
        }
    }
}

/// Eventos push (daemon → cliente). Preparado no protocolo; usado na Fase 2+.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Event {
    pub event: String,
    pub data: serde_json::Value,
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::error::{ErrorDomain, SysforgeError};

    #[test]
    fn request_response_roundtrip() {
        let req = Request {
            id: 7,
            method: METHOD_ECHO.to_string(),
            params: serde_json::json!({"message": "oi"}),
        };
        let line = serde_json::to_string(&req).unwrap();
        let back: Request = serde_json::from_str(&line).unwrap();
        assert_eq!(back.id, 7);
        assert_eq!(back.method, "v1.echo");
        assert_eq!(back.params["message"], "oi");

        let resp = Response::ok(7, serde_json::json!({"echo": "oi"}));
        let line = serde_json::to_string(&resp).unwrap();
        assert!(line.contains("\"ok\":true"));
        assert!(!line.contains("error"));
        let back: Response = serde_json::from_str(&line).unwrap();
        assert_eq!(back.id, 7);
    }

    #[test]
    fn wire_error_maps_both_ways() {
        let e = SysforgeError::new(ErrorDomain::Auth, 2, "destrutivo recusado")
            .with_recommendation("use o daemon em modo sistema");
        let w: WireError = e.clone().into();
        assert_eq!(w.code, "SF-AUTH-002");
        let back: SysforgeError = w.into();
        assert_eq!(back.code, "SF-AUTH-002");
        assert_eq!(back.recommendation, "use o daemon em modo sistema");
    }

    #[test]
    fn destructive_registry_is_declared() {
        // O fail-closed é contra ESTAS listas — explícitas e testáveis.
        assert!(DESTRUCTIVE_REGISTRY.contains(&"v1.disk.wipe"));
        assert!(!DESTRUCTIVE_REGISTRY.contains(&"v1.boot.set_next"),
            "set_next agora é privilegiado IMPLEMENTADO (LowRisk + polkit)");
        assert!(PRIVILEGED_METHODS.contains(&METHOD_BOOT_SET_NEXT));
        assert!(PRIVILEGED_METHODS.contains(&METHOD_SYSTEM_POWEROFF));
    }
}
