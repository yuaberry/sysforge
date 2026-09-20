//! Protocolo IPC do daemon `yua-osd`: NDJSON (uma mensagem JSON por linha)
//! sobre Unix socket. Versão v1, métodos read-only.
//!
//! Regras de segurança:
//! - O daemon SEMPRE conhece o uid do chamador via SO_PEERCRED (sem confiar
//!   em nada que o cliente envie).
//! - Modo dev: mesmo uid ⇒ read-only OK; destrutivo recusado (fail-closed).
//! - Modo sistema: autorização por método via polkit (pkcheck).
//! - Erros carregam código YUA-XXX-NNN — nada de "erro genérico".

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::error::YuaError;

pub const PROTOCOL_VERSION: u32 = 1;

// ---- Métodos v1 (todos read-only) ----
pub const METHOD_ECHO: &str = "v1.echo";
pub const METHOD_SYSTEM_INFO: &str = "v1.system.info";
pub const METHOD_DISKS_LIST: &str = "v1.disks.list";
pub const METHOD_EFI_ENTRIES: &str = "v1.efi.entries";
pub const METHOD_CAPABILITIES: &str = "v1.capabilities";
pub const METHOD_DAEMON_INFO: &str = "v1.daemon.info";

/// Métodos que UM DIA serão destrutivos. Na Fase 1 são recusados em TODOS
/// os modos — o registro existe para o fail-closed ser explícito e testável.
pub const DESTRUCTIVE_REGISTRY: &[&str] = &[
    "v1.disk.wipe",
    "v1.disk.format",
    "v1.deploy.start",
    "v1.boot.set_next",
];

/// Socket do daemon em modo sistema (systemd socket activation).
pub const DEFAULT_SYSTEM_SOCKET: &str = "/run/yua-osd.sock";

/// Socket dev padrão: $XDG_RUNTIME_DIR/yua-osd.dev.sock (ou /tmp).
pub fn dev_socket_default() -> PathBuf {
    if let Some(xdg) = std::env::var_os("XDG_RUNTIME_DIR") {
        PathBuf::from(xdg).join("yua-osd.dev.sock")
    } else {
        PathBuf::from("/tmp").join(format!("yua-osd.dev.{}.sock", std::env::var("UID").unwrap_or_else(|_| "0".into())))
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

impl From<YuaError> for WireError {
    fn from(e: YuaError) -> Self {
        Self {
            code: e.code,
            message: e.message,
            technical: e.technical,
            recommendation: e.recommendation,
        }
    }
}

impl From<WireError> for YuaError {
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
    use crate::error::{ErrorDomain, YuaError};

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
        let e = YuaError::new(ErrorDomain::Auth, 2, "destrutivo recusado")
            .with_recommendation("use o daemon em modo sistema");
        let w: WireError = e.clone().into();
        assert_eq!(w.code, "YUA-AUTH-002");
        let back: YuaError = w.into();
        assert_eq!(back.code, "YUA-AUTH-002");
        assert_eq!(back.recommendation, "use o daemon em modo sistema");
    }

    #[test]
    fn destructive_registry_is_declared() {
        // O fail-closed da Fase 1 é contra ESTA lista — explícita e testável.
        assert!(DESTRUCTIVE_REGISTRY.contains(&"v1.disk.wipe"));
        assert!(DESTRUCTIVE_REGISTRY.contains(&"v1.boot.set_next"));
    }
}
