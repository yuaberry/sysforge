//! Servidor IPC: Unix socket + SO_PEERCRED + auditoria de cada chamada.

use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::os::unix::fs::PermissionsExt;
use std::os::unix::io::AsRawFd;
use std::os::unix::net::{UnixListener, UnixStream};
use std::path::Path;
use std::thread;

use chrono::Utc;
use serde_json::json;

use yua_core::error::{ErrorDomain, YuaError};
use yua_core::ipc::protocol::{Request, Response, WireError};

use crate::handlers;
use crate::{auth, Config, DaemonMode};

#[derive(Debug, Clone, Copy)]
pub struct Peer {
    pub pid: u32,
    pub uid: u32,
}

/// Identidade REAL do cliente direto do kernel. O cliente não envia nada
/// que possa mentir aqui — é o socket que reporta.
fn peer_credentials(stream: &UnixStream) -> Result<Peer, YuaError> {
    let mut ucred: libc::ucred = unsafe { std::mem::zeroed() };
    let mut len = std::mem::size_of::<libc::ucred>() as libc::socklen_t;
    let rc = unsafe {
        libc::getsockopt(
            stream.as_raw_fd(),
            libc::SOL_SOCKET,
            libc::SO_PEERCRED,
            &mut ucred as *mut _ as *mut libc::c_void,
            &mut len,
        )
    };
    if rc != 0 {
        return Err(YuaError::new(
            ErrorDomain::Auth,
            3,
            "Não foi possível obter a identidade (SO_PEERCRED) do cliente",
        ));
    }
    Ok(Peer {
        pid: ucred.pid as u32,
        uid: ucred.uid,
    })
}

fn audit_path(mode: DaemonMode) -> Option<std::path::PathBuf> {
    match mode {
        // dev: dentro do perfil do usuário
        DaemonMode::Dev => std::env::var_os("HOME").map(|h| {
            std::path::PathBuf::from(h).join(".local/share/yua-os-manager/logs/audit.jsonl")
        }),
        // system: área do serviço (root)
        DaemonMode::System => Some(std::path::PathBuf::from(
            "/var/lib/yua-os-manager/audit.jsonl",
        )),
    }
}

/// Auditoria: TODA chamada de método é registrada — permitida ou negada.
fn audit(mode: DaemonMode, peer: Peer, method: &str, allowed: bool) {
    let Some(path) = audit_path(mode) else { return };
    if let Some(dir) = path.parent() {
        let _ = fs::create_dir_all(dir);
    }
    let entry = json!({
        "ts": Utc::now().to_rfc3339(),
        "peer_pid": peer.pid,
        "peer_uid": peer.uid,
        "method": method,
        "allowed": allowed,
    });
    if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(&path) {
        let _ = writeln!(f, "{entry}");
    }
}

fn prepare_socket(path: &Path, mode: DaemonMode) -> Result<UnixListener, YuaError> {
    if path.exists() {
        // Socket de instância viva? Recusa duplicação. Morto (stale)? Remove.
        match UnixStream::connect(path) {
            Ok(_) => {
                return Err(YuaError::new(
                    ErrorDomain::Io,
                    10,
                    "Outra instância do yua-osd já está escutando neste socket",
                )
                .with_technical(format!("socket: {}", path.display()))
                .with_recommendation("Use a instância existente ou pare-a antes de iniciar outra."))
            }
            Err(_) => fs::remove_file(path)?,
        }
    }
    if let Some(dir) = path.parent() {
        fs::create_dir_all(dir)?;
    }
    let listener = UnixListener::bind(path)?;
    let perms = match mode {
        // dev: apenas o dono; system: qualquer usuário conecta (auth é por método)
        DaemonMode::Dev => fs::Permissions::from_mode(0o600),
        DaemonMode::System => fs::Permissions::from_mode(0o666),
    };
    fs::set_permissions(path, perms)?;
    Ok(listener)
}

pub fn serve(cfg: &Config) -> Result<(), YuaError> {
    let listener = prepare_socket(&cfg.socket, cfg.mode)?;
    tracing::info!(socket = %cfg.socket.display(), mode = cfg.mode.label(), "aguardando conexões IPC");

    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                let cfg = cfg.clone();
                thread::spawn(move || {
                    if let Err(e) = handle_connection(s, &cfg) {
                        tracing::warn!(code = %e.code, "conexão encerrou com erro: {e}");
                    }
                });
            }
            Err(e) => {
                tracing::warn!("accept falhou: {e}");
                continue;
            }
        }
    }
    Ok(())
}

fn handle_connection(stream: UnixStream, cfg: &Config) -> Result<(), YuaError> {
    let peer = peer_credentials(&stream)?;
    let reader = BufReader::new(stream.try_clone()?);
    let mut writer = stream;

    for line in reader.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let resp = match serde_json::from_str::<Request>(&line) {
            Ok(req) => {
                let decision = auth::authorize(cfg.mode, peer.uid, &req.method);
                let allowed = matches!(decision, auth::Decision::Allow);
                audit(cfg.mode, peer, &req.method, allowed);
                match decision {
                    auth::Decision::Allow => {
                        tracing::info!(uid = peer.uid, pid = peer.pid, method = %req.method, "chamada");
                        handlers::dispatch(&req, cfg)
                    }
                    auth::Decision::Deny(e) => {
                        tracing::warn!(uid = peer.uid, pid = peer.pid, method = %req.method, code = %e.code, "recusada");
                        Response::err(req.id, e)
                    }
                }
            }
            Err(e) => Response::err(
                0,
                WireError::from(
                    YuaError::new(ErrorDomain::Io, 11, "Requisição malformada no protocolo NDJSON")
                        .with_technical(e.to_string()),
                ),
            ),
        };
        writeln!(writer, "{}", serde_json::to_string(&resp)?)?;
        writer.flush()?;
    }
    Ok(())
}
