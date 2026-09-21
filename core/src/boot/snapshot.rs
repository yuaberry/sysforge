//! Snapshot do estado UEFI — foto completa ANTES de qualquer mutação.
//! Regra da plataforma: nenhuma escrita em boot acontece sem snapshot prévio.

use std::fs;
use std::path::PathBuf;

use chrono::Utc;
use serde::{Deserialize, Serialize};

use crate::boot::efi::{read_efi_state, EfiBootState};
use crate::error::YuaError;
use crate::executor::{CommandSpec, Executor};

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct BootSnapshot {
    pub captured_at: String,
    /// Estado parseado (ordem, entradas, bootnext).
    pub state: EfiBootState,
    /// Saída crua de `efibootmgr -v` — usada para auditoria/rollback manual.
    pub raw_verbose: String,
}

impl BootSnapshot {
    /// Captura o estado ATUAL (read-only — funciona até em modo dev).
    pub fn capture(executor: &Executor) -> Result<Self, YuaError> {
        let state = read_efi_state(executor)?;
        let spec = CommandSpec::new("efibootmgr").arg("-v").timeout(std::time::Duration::from_secs(15));
        let raw = executor.run_readonly(spec)?;
        Ok(Self {
            captured_at: Utc::now().to_rfc3339(),
            state,
            raw_verbose: raw.stdout,
        })
    }

    /// Persiste o snapshot com escrita atômica; devolve o caminho gravado.
    /// Dev mode → estado do usuário; system mode → /var/lib.
    pub fn save(&self) -> Result<PathBuf, YuaError> {
        let dir = snapshots_dir()?;
        fs::create_dir_all(&dir)?;
        let ts = Utc::now().format("%Y%m%d-%H%M%S");
        let name = format!("boot-snapshot-{ts}.json");
        let path = dir.join(&name);
        let tmp = dir.join(format!(".{name}.tmp"));
        let json = serde_json::to_vec_pretty(self)?;
        fs::write(&tmp, &json)?;
        fs::rename(&tmp, &path)?;
        tracing::info!(snapshot = %path.display(), "estado UEFI fotografado");
        Ok(path)
    }
}

fn snapshots_dir() -> Result<PathBuf, YuaError> {
    if unsafe { libc::getuid() } == 0 {
        Ok(PathBuf::from("/var/lib/yua-os-manager/snapshots"))
    } else {
        let home = std::env::var_os("HOME")
            .map(PathBuf::from)
            .ok_or_else(|| YuaError::new(crate::error::ErrorDomain::Io, 2, "HOME não definido"))?;
        Ok(home.join(".local/share/yua-os-manager/snapshots"))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn capture_real_uefi_state() {
        // Máquina real: snapshot captura as 14 entradas UEFI de verdade.
        let ex = Executor::default();
        let snap = BootSnapshot::capture(&ex).unwrap();
        assert!(snap.state.entries.len() >= 13);
        assert!(snap.raw_verbose.contains("Boot0000"));
        assert!(snap.captured_at.contains('T'));
    }

    #[test]
    fn save_writes_atomic_json() {
        let ex = Executor::default();
        let snap = BootSnapshot::capture(&ex).unwrap();
        let path = snap.save().unwrap();
        let raw = std::fs::read_to_string(&path).unwrap();
        let back: BootSnapshot = serde_json::from_str(&raw).unwrap();
        assert_eq!(back.state.entries.len(), snap.state.entries.len());
        std::fs::remove_file(&path).ok();
    }
}
