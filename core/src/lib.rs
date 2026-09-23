//! # sysforge-core — núcleo do SYSFORGE
//!
//! Biblioteca central compartilhada por GUI (Tauri), daemon (`sysforge-osd`) e CLI (`sysforge`).
//!
//! Princípios deste crate:
//! - **Nenhum código de instalação falso**: o que não existe, recusa com código de erro.
//! - **Comandos sempre `program + args`**: nunca string de shell (anti-injection).
//! - **Operações destrutivas só existem com identidade de disco revalidada**.
//! - **Fail-closed**: sem autorização (polkit), destrutivo NUNCA roda.

pub mod error;
pub mod executor;
pub mod state;
pub mod capability;
pub mod logging;
pub mod hw;
pub mod disk;
pub mod boot;
pub mod power;
pub mod windows;
pub mod ipc;

pub const YUA_VERSION: &str = env!("CARGO_PKG_VERSION");

/// Estado global de disponibilidade de um recurso.
/// `Unavailable` SEMPRE carrega o motivo técnico — nunca "não funciona" seco.
#[derive(Debug, Clone, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Availability {
    Available,
    Unavailable {
        /// Código SF-XXX-NNN explicando a indisponibilidade.
        reason_code: String,
        /// Motivo técnico legível (ex.: "smartctl requer root").
        reason: String,
    },
}

impl Availability {
    pub fn unavailable(code: &str, reason: impl Into<String>) -> Self {
        Self::Unavailable {
            reason_code: code.to_string(),
            reason: reason.into(),
        }
    }

    pub fn is_available(&self) -> bool {
        matches!(self, Availability::Available)
    }
}
