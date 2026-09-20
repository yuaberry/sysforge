//! Identidade de disco com revalidação — a barreira anti-hot-plug.
//!
//! CENÁRIO QUE ISTO EVITA: o técnico inicia a operação apontando para
//! /dev/sdb; ele conecta um pendrive; o kernel renumera; /dev/sdb vira
//! OUTRO disco; um `wipefs` às cegas apagaria o disco errado. Por isso
//! toda operação destrutiva carrega a `DiskIdentity` do PLANEJAMENTO e
//! revalida modelo+serial+ tamanho MILISSEGUNDOS antes de executar.
//! Qualquer divergência → YUA-DISK-009 → operação abortada.

use serde::{Deserialize, Serialize};

use crate::disk::lsblk::{self, LsblkDevice};
use crate::disk::udev::enrich_from_udev;
use crate::error::{ErrorDomain, YuaError};
use crate::executor::Executor;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct DiskIdentity {
    /// Caminho canônico, ex.: /dev/sda
    pub path: String,
    pub model: Option<String>,
    pub serial: Option<String>,
    pub size_bytes: u64,
    /// Fixture de teste — nunca existe de verdade; revalidação é no-op.
    #[serde(default, skip_serializing)]
    pub synthetic: bool,
}

impl DiskIdentity {
    /// Sondagem real via lsblk (+ enriquecimento udev para serial/model).
    pub fn probe(executor: &Executor, path: &str) -> Result<Self, YuaError> {
        let mut dev = lsblk::probe_device(executor, path)?;
        enrich_from_udev(&mut dev);
        Ok(Self::from_device(&dev))
    }

    pub fn from_device(dev: &LsblkDevice) -> Self {
        Self {
            path: dev.path.clone().unwrap_or_else(|| format!("/dev/{}", dev.name)),
            model: dev.model_trimmed(),
            serial: dev.serial_trimmed(),
            size_bytes: dev.size,
            synthetic: false,
        }
    }

    /// Constrói identidade a partir de um disco do inventário (sem nova
    /// sondagem) — usado no planejamento com dados do lsblk já coletados.
    pub fn from_disk(dev: &LsblkDevice) -> Self {
        Self::from_device(dev)
    }

    #[doc(hidden)]
    pub fn synthetic_for_tests(path: &str) -> Self {
        Self {
            path: path.to_string(),
            model: Some("SYNTHETIC DISK".into()),
            serial: Some("SYNTHETIC-SERIAL".into()),
            size_bytes: 1 << 30,
            synthetic: true,
        }
    }

    /// Revalida a identidade AGORA (nova sondagem) contra a registrada.
    /// Divergência de model/serial/size → YUA-DISK-009 (abortar).
    pub fn revalidate_now(&self) -> Result<(), YuaError> {
        if self.synthetic {
            tracing::warn!(disk = %self.path, "revalidate_now chamado em identidade sintética (fixture de teste)");
            return Ok(());
        }
        let exec = Executor::default();
        let current = Self::probe(&exec, &self.path)?;
        if current.model != self.model
            || current.serial != self.serial
            || current.size_bytes != self.size_bytes
        {
            return Err(YuaError::new(
                ErrorDomain::Disk,
                9,
                format!(
                    "A identidade do disco {} mudou desde o planejamento — operação abortada",
                    self.path
                ),
            )
            .with_technical(format!(
                "planejado: model={:?} serial={:?} size={}; atual: model={:?} serial={:?} size={}",
                self.model, self.serial, self.size_bytes,
                current.model, current.serial, current.size_bytes
            ))
            .with_recommendation(
                "Isto indica hot-plug/renumeração de dispositivos. Redimensione o plano com o novo inventário de discos antes de prosseguir.",
            ));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_real_host_disk_has_wdc_identity() {
        // Máquina real: o disco do rootfs é o WDC com serial conhecido.
        let ex = Executor::default();
        let id = DiskIdentity::probe(&ex, "/dev/sda").unwrap();
        assert_eq!(id.serial.as_deref(), Some("WX61A79A2TDH"));
        assert_eq!(id.size_bytes, 1000204886016);
        assert!(!id.synthetic);
    }

    #[test]
    fn revalidate_synthetic_is_noop() {
        let id = DiskIdentity::synthetic_for_tests("/dev/zzz9");
        assert!(id.revalidate_now().is_ok());
    }

    #[test]
    fn probe_missing_disk_is_disk_001() {
        let ex = Executor::default();
        let err = DiskIdentity::probe(&ex, "/dev/nonexistent-disk-x").unwrap_err();
        assert_eq!(err.code, "YUA-DISK-001");
    }
}
