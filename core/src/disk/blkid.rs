//! Sondagem de filesystem via `blkid -o export` (formato KEY=VALUE).

use std::collections::BTreeMap;

use crate::error::YuaError;
use crate::executor::{CommandSpec, Executor};

/// Parser do formato export do blkid:
/// ```text
/// /dev/sda1: UUID="C065-50C0" VERSION="FAT32" TYPE="vfat"
/// ```
/// (com -o export cada atributo vem em linha própria: `KEY="value"`).
pub fn parse_blkid_export(output: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in output.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        // Com -o export o blkid emite linhas "DEVNAME=/dev/sda1", "UUID=...",
        // cada chave uma vez; com saída clássica emite "dev: KEY=V ...".
        for token in line.split_whitespace() {
            if let Some((k, v)) = token.split_once('=') {
                map.insert(k.trim().to_string(), v.trim_matches('"').to_string());
            }
        }
    }
    map
}

/// Sonda um dispositivo específico. Pode exigir permissões para dispositivos
/// não montados — o erro sobe honesto para o chamador decidir.
pub fn probe_blkid(executor: &Executor, device: &str) -> Result<BTreeMap<String, String>, YuaError> {
    let spec = CommandSpec::new("blkid")
        .arg("--output")
        .arg("export")
        .arg(device)
        .timeout(std::time::Duration::from_secs(15));
    let r = executor.run_readonly(spec)?;
    if !r.success() {
        return Err(YuaError::command_failed("blkid", &[], r.exit_code, &r.stderr));
    }
    Ok(parse_blkid_export(&r.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_export_format() {
        let sample = "DEVNAME=/dev/sda1\nLABEL=\"ESP\"\nUUID=\"C065-50C0\"\nVERSION=\"FAT32\"\nTYPE=\"vfat\"\nUSAGE=\"filesystem\"\n";
        let m = parse_blkid_export(sample);
        assert_eq!(m.get("TYPE").unwrap(), "vfat");
        assert_eq!(m.get("UUID").unwrap(), "C065-50C0");
        assert_eq!(m.get("DEVNAME").unwrap(), "/dev/sda1");
    }

    #[test]
    fn live_probe_esp() {
        // Máquina real: a ESP /dev/sda1 é vfat (FAT32) — read-only, sem root.
        let ex = Executor::default();
        let m = probe_blkid(&ex, "/dev/sda1").unwrap();
        assert_eq!(m.get("TYPE").map(String::as_str), Some("vfat"));
    }
}
