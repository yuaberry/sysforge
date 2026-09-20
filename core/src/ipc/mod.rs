//! Protocolo IPC YUA (NDJSON sobre Unix socket) + cliente.

pub mod client;
pub mod protocol;

pub use client::YuaClient;
pub use protocol::{
    Request, Response, WireError, DEFAULT_SYSTEM_SOCKET, METHOD_CAPABILITIES, METHOD_DAEMON_INFO,
    METHOD_DISKS_LIST, METHOD_ECHO, METHOD_EFI_ENTRIES, METHOD_SYSTEM_INFO, dev_socket_default,
};
