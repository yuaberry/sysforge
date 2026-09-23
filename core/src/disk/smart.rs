//! Saúde de disco via smartctl — COM HONESTIDADE RADICAL:
//!
//! - smartctl não instalado → indisponível (SF-DEP-005) + como instalar.
//! - instalado mas sem root → indisponível (SF-DEP-006) + por quê.
//! NUNCA inventamos valores de saúde. Sem dado real, o relatório diz isso.

use serde::Serialize;

use crate::executor::{which, CommandSpec, Executor};
use crate::Availability;

#[derive(Debug, Clone, Serialize)]
pub struct SmartReport {
    pub device: String,
    pub available: Availability,
    /// SMART overall-passed (true = disco saudável segundo o SMART).
    pub passed: Option<bool>,
    pub temperature_c: Option<i32>,
    pub power_on_hours: Option<u64>,
    /// Setores realocados (atributo ID 5) — preditor clássico de falha.
    pub reallocated_sectors: Option<u64>,
}

pub fn smart_health(executor: &Executor, device: &str) -> SmartReport {
    let mut report = SmartReport {
        device: device.to_string(),
        available: Availability::Available,
        passed: None,
        temperature_c: None,
        power_on_hours: None,
        reallocated_sectors: None,
    };

    if which("smartctl").is_none() {
        report.available = Availability::unavailable(
            "SF-DEP-005",
            "smartctl (pacote smartmontools) não está instalado neste sistema",
        );
        return report;
    }

    let spec = CommandSpec::new("smartctl")
        .arg("--json")
        .arg("--health")
        .arg("--attributes")
        .arg(device)
        .timeout(std::time::Duration::from_secs(30));
    let result = executor.run_readonly(spec);

    let result = match result {
        Ok(r) => r,
        Err(e) => {
            report.available = Availability::unavailable("SF-DEP-007", e.message.clone());
            return report;
        }
    };

    // smartctl exige root para abrir o dispositivo de bloco.
    if !result.success() {
        let stderr = result.stderr.to_lowercase();
        let stdout = result.stdout.to_lowercase();
        if stderr.contains("permission denied")
            || stdout.contains("permission denied")
            || stderr.contains("root")
        {
            report.available = Availability::unavailable(
                "SF-DEP-006",
                "smartctl requer root para ler este dispositivo",
            );
            return report;
        }
        // Alguns exit codes do smartctl (ex.: bit 0 = falha de cmdline) ainda
        // trazem JSON válido no stdout; tentamos parsear mesmo assim.
    }

    match serde_json::from_str::<serde_json::Value>(&result.stdout) {
        Ok(obj) => {
            report.passed = obj
                .pointer("/smart_status/passed")
                .and_then(|v| v.as_bool());
            report.temperature_c = obj
                .pointer("/temperature/current")
                .and_then(|v| v.as_i64())
                .map(|v| v as i32);
            report.power_on_hours = obj
                .pointer("/power_on_time/hours")
                .and_then(|v| v.as_u64());
            if let Some(table) = obj
                .pointer("/ata_smart_attributes/table")
                .and_then(|v| v.as_array())
            {
                for attr in table {
                    if attr.get("id").and_then(|v| v.as_u64()) == Some(5) {
                        report.reallocated_sectors = attr
                            .pointer("/raw/value")
                            .and_then(|v| v.as_u64());
                    }
                }
            }
            if report.available.is_available() && report.passed.is_none() && result.stdout.trim().is_empty() {
                report.available =
                    Availability::unavailable("SF-DEP-008", "smartctl não retornou dados legíveis");
            }
        }
        Err(e) => {
            report.available = Availability::unavailable(
                "SF-DEP-008",
                format!("saída do smartctl não pôde ser interpretada: {e}"),
            );
        }
    }
    report
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn honest_unavailability_without_smartctl_installed() {
        // Nesta máquina smartmontools NÃO está instalado — o engine deve
        // reportar indisponibilidade honesta com código e motivo, nunca
        // inventar valores.
        let ex = Executor::default();
        let r = smart_health(&ex, "/dev/sda");
        assert!(!r.available.is_available(), "smartctl ausente ⇒ unavailable");
        assert_eq!(
            match &r.available {
                crate::Availability::Unavailable { reason_code, .. } => reason_code.clone(),
                _ => String::new(),
            },
            "SF-DEP-005"
        );
        assert!(r.passed.is_none() && r.temperature_c.is_none());
    }

    #[test]
    fn never_panics_on_fake_device() {
        let ex = Executor::default();
        let _ = smart_health(&ex, "/dev/nonexistent-disk");
    }
}
