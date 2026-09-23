//! Máquina de estados da operação de deployment.
//!
//! POR QUE uma máquina de estados: uma instalação atravessa um reboot.
//! Sequências soltas de booleans não sobrevivem a queda de energia. O estado
//! é persistido com escrita atômica (tmp + fsync + rename) e checksum
//! SHA-256 próprio; ao reiniciar o app, detecta operação interrompida e
//! **re-verifica** antes de permitir retomar — nunca retoma cegamente.

use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};

use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::error::{ErrorDomain, SysforgeError};

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum OperationState {
    Idle,
    Planning,
    Validating,
    Staging,
    Ready,
    Rebooting,
    BootedRecovery,
    Deploying,
    Configuring,
    Verifying,
    Completed,
    #[serde(rename = "failed")]
    Failed,
    #[serde(rename = "cancelled")]
    Cancelled,
    #[serde(rename = "rollback")]
    Rollback,
}

impl OperationState {
    pub fn label(self) -> &'static str {
        match self {
            OperationState::Idle => "IDLE",
            OperationState::Planning => "PLANNING",
            OperationState::Validating => "VALIDATING",
            OperationState::Staging => "STAGING",
            OperationState::Ready => "READY",
            OperationState::Rebooting => "REBOOTING",
            OperationState::BootedRecovery => "BOOTED_RECOVERY",
            OperationState::Deploying => "DEPLOYING",
            OperationState::Configuring => "CONFIGURING",
            OperationState::Verifying => "VERIFYING",
            OperationState::Completed => "COMPLETED",
            OperationState::Failed => "FAILED",
            OperationState::Cancelled => "CANCELLED",
            OperationState::Rollback => "ROLLBACK",
        }
    }

    /// Transições legais do fluxo (grafo explícito, não convenções).
    ///
    /// Regras:
    /// - Estados **terminais** (Completed/Failed/Cancelled/Rollback) nunca
    ///   transicionam por aqui. Retomadas pós-reboot são decisões explícitas
    ///   da operação de recovery, não transições automáticas.
    /// - Qualquer estado **ativo** pode falhar, ser cancelado ou iniciar
    ///   rollback (o rollback é sempre uma saída de segurança).
    /// - O caminho feliz é uma sequência estrita; estados anteriores só
    ///   podem retroceder um passo (ex.: Staging → Validating) para
    ///   re-planejar com dados reais.
    pub fn can_transition(self, to: OperationState) -> bool {
        use OperationState::*;
        if matches!(self, Completed | Failed | Cancelled | Rollback) {
            return false;
        }
        if matches!(to, Failed | Cancelled | Rollback) {
            return true;
        }
        matches!(
            (self, to),
            (Idle, Planning)
                | (Planning, Validating | Idle)
                | (Validating, Staging | Planning | Idle)
                | (Staging, Ready | Validating)
                | (Ready, Rebooting | Staging)
                | (Rebooting, BootedRecovery)
                | (BootedRecovery, Deploying)
                | (Deploying, Configuring)
                | (Configuring, Verifying)
                | (Verifying, Completed)
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct TransitionEntry {
    pub from: OperationState,
    pub to: OperationState,
    pub at: DateTime<Utc>,
    pub note: Option<String>,
}

/// Registro persistido da operação (espelha /var/lib/sysforge/operations).
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct OperationRecord {
    pub operation_id: String,
    pub state: OperationState,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
    /// Modo de instalação (express, advanced, automated, recovery, custom).
    pub mode: Option<String>,
    /// Alvo (ex.: "windows-11-25h2").
    pub target: Option<String>,
    /// Disco destino em /dev/... (quando aplicável).
    pub disk: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub expected_next_boot: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub rollback_possible: Option<bool>,
    pub transitions: Vec<TransitionEntry>,
    /// Checksum SHA-256 do conteúdo (sem o próprio campo).
    pub checksum: String,
}

impl OperationRecord {
    pub fn new(operation_id: impl Into<String>) -> Self {
        let mut rec = Self {
            operation_id: operation_id.into(),
            state: OperationState::Idle,
            created_at: Utc::now(),
            updated_at: Utc::now(),
            mode: None,
            target: None,
            disk: None,
            expected_next_boot: None,
            rollback_possible: None,
            transitions: Vec::new(),
            checksum: String::new(),
        };
        // Todo registro legítimo NASCE com checksum — load() compara
        // estritamente; checksum ausente/zerado é adulteração, não legado.
        rec.checksum = rec.compute_checksum();
        rec
    }

    /// Calcula o checksum sobre o JSON canônico sem o campo checksum.
    pub fn compute_checksum(&self) -> String {
        let mut copy = self.clone();
        copy.checksum = String::new();
        let json = serde_json::to_string(&copy).unwrap_or_default();
        let mut h = Sha256::new();
        h.update(json.as_bytes());
        format!("{:x}", h.finalize())
    }

    /// Transição validada + registrada no histórico.
    pub fn transition(
        &mut self,
        to: OperationState,
        note: Option<String>,
    ) -> Result<(), SysforgeError> {
        if !self.state.can_transition(to) {
            return Err(SysforgeError::new(
                ErrorDomain::State,
                2,
                format!(
                    "Transição inválida de {} para {}",
                    self.state.label(),
                    to.label()
                ),
            )
            .with_technical("a máquina de estados do deployment só permite transições do grafo explícito")
            .with_recommendation("Se isto ocorrer, a operação está em estado inconsistente: consulte os logs e use `sysforge recover`."));
        }
        let entry = TransitionEntry {
            from: self.state,
            to,
            at: Utc::now(),
            note,
        };
        tracing::info!(op = %self.operation_id, from = self.state.label(), to = entry.to.label(), "state transition");
        self.state = to;
        self.updated_at = entry.at;
        self.transitions.push(entry);
        self.checksum = self.compute_checksum();
        Ok(())
    }

    /// Persistência atômica: tmp + fsync + rename. Se o processo morrer no
    /// meio, o arquivo antigo permanece íntegro — nunca um JSON truncado.
    pub fn persist_atomic(&self, dir: &Path) -> Result<PathBuf, SysforgeError> {
        fs::create_dir_all(dir)?;
        let final_path = dir.join(format!("{}.operation.json", self.operation_id));
        let tmp_path = dir.join(format!(".{}.operation.json.tmp", self.operation_id));
        let json = serde_json::to_vec_pretty(self)?;
        {
            let mut f = fs::File::create(&tmp_path)?;
            f.write_all(&json)?;
            f.sync_all()?;
        }
        fs::rename(&tmp_path, &final_path)?;
        Ok(final_path)
    }

    /// Carrega e valida o checksum (detecta corrupção/edição manual).
    /// Comparação ESTRITA: checksum ausente também é adulteração.
    pub fn load(path: &Path) -> Result<Self, SysforgeError> {
        let raw = fs::read_to_string(path)?;
        let rec: OperationRecord = serde_json::from_str(&raw)?;
        let expected = rec.compute_checksum();
        if rec.checksum != expected {
            return Err(SysforgeError::new(
                ErrorDomain::State,
                3,
                "Arquivo de operação corrompido (checksum divergente)",
            )
            .with_technical(format!("esperado {expected}, encontrado {}", rec.checksum))
            .with_recommendation("Não prossiga automaticamente. Inspecione o arquivo e os logs antes de retomar."));
        }
        Ok(rec)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn happy_path_transitions() {
        let mut op = OperationRecord::new("op-happy");
        for to in [
            OperationState::Planning,
            OperationState::Validating,
            OperationState::Staging,
            OperationState::Ready,
            OperationState::Rebooting,
            OperationState::BootedRecovery,
            OperationState::Deploying,
            OperationState::Configuring,
            OperationState::Verifying,
            OperationState::Completed,
        ] {
            op.transition(to, None).unwrap_or_else(|e| {
                panic!("transição {} -> {:?} deveria ser legal: {e}", op.state.label(), to)
            });
        }
        assert_eq!(op.state, OperationState::Completed);
    }

    #[test]
    fn invalid_transitions_rejected() {
        let mut op = OperationRecord::new("op-bad");
        assert!(op.transition(OperationState::Deploying, None).is_err());
        op.transition(OperationState::Planning, None).unwrap();
        assert!(op.transition(OperationState::Completed, None).is_err());
        // qualquer estado ativo pode falhar
        op.transition(OperationState::Failed, None).unwrap();
        // ...mas de FAILED não se segue direto para COMPLETED
        assert!(op.transition(OperationState::Completed, None).is_err());
    }

    #[test]
    fn atomic_persist_and_load_roundtrip() {
        let dir = std::env::temp_dir().join(format!("sysforge-state-test-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut op = OperationRecord::new("op-persist");
        op.transition(OperationState::Planning, Some("teste".into())).unwrap();
        let path = op.persist_atomic(&dir).unwrap();
        let loaded = OperationRecord::load(&path).unwrap();
        assert_eq!(loaded.state, OperationState::Planning);
        assert_eq!(loaded.transitions.len(), 1);
        assert_eq!(loaded.checksum, loaded.compute_checksum());
        fs::remove_dir_all(&dir).ok();
    }

    #[test]
    fn checksum_detects_tamper() {
        let dir = std::env::temp_dir().join(format!("sysforge-state-tamper-{}", std::process::id()));
        fs::create_dir_all(&dir).unwrap();
        let mut op = OperationRecord::new("op-tamper");
        op.persist_atomic(&dir).unwrap();
        let path = dir.join("op-tamper.operation.json");
        let raw = fs::read_to_string(&path).unwrap();
        let tampered = raw.replace("\"state\": \"idle\"", "\"state\": \"deploying\"");
        fs::write(&path, tampered).unwrap();
        assert_eq!(OperationRecord::load(&path).unwrap_err().code, "SF-STATE-003");
        fs::remove_dir_all(&dir).ok();
    }
}
