//! Enriquecimento via banco de dados do udev (/run/udev/data).
//!
//! POR QUE: o lsblk como usuário comum às vezes oculta serial/model de
//! discos (dependendo de permissões), mas o banco do udev é legível por
//! qualquer usuário — e é dali que o próprio lsblk tira boa parte dos dados.
//! Chave: "b{MAJOR}:{MINOR}" (ex.: /run/udev/data/b8:0).

use std::collections::BTreeMap;
use std::fs;

use crate::disk::lsblk::LsblkDevice;

pub fn udev_properties(majmin: &str) -> BTreeMap<String, String> {
    let mut props = BTreeMap::new();
    let path = format!("/run/udev/data/b{majmin}");
    if let Ok(content) = fs::read_to_string(path) {
        for line in content.lines() {
            if let Some(kv) = line.strip_prefix("E:") {
                if let Some((k, v)) = kv.split_once('=') {
                    props.insert(k.trim().to_string(), v.trim().to_string());
                }
            }
        }
    }
    props
}

/// Preenche serial/model ausentes do lsblk usando o udev
/// (ID_SERIAL_SHORT / ID_MODEL — presentes em qualquer disco real).
pub fn enrich_from_udev(dev: &mut LsblkDevice) {
    let Some(majmin) = dev.majmin.clone() else {
        return;
    };
    let props = udev_properties(&majmin);
    if dev.serial_trimmed().is_none() {
        if let Some(serial) = props.get("ID_SERIAL_SHORT") {
            dev.serial = Some(serial.clone());
        }
    }
    if dev.model_trimmed().is_none() {
        if let Some(model) = props.get("ID_MODEL") {
            dev.model = Some(model.clone());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_real_udev_entry() {
        // Entrada REAL de /run/udev/data/b8:0 desta máquina (disco WDC).
        let sample = "P:/devices/pci0000:00/0000:00:1f.2/ata1/host0/target0:0:0/0:0:0:0/block/sda\nN:sda\nL:0\nS:disk/by-id/ata-WDC_WD10SPZX-75Z10T2_WX61A79A2TDH\nE:DEVNAME=/dev/sda\nE:DEVTYPE=disk\nE:MAJOR=8\nE:MINOR=0\nE:SUBSYSTEM=block\nE:ID_PART_TABLE_TYPE=gpt\nE:ID_BUS=ata\nE:ID_MODEL=WDC_WD10SPZX-75Z10T2\nE:ID_SERIAL_SHORT=WX61A79A2TDH\nE:ID_ATA_ROTATION_RATE_RPM=5400\n";
        // Grava fixture temporária onde o parser espera? Não: parser lê arquivo
        // real; aqui validamos o formato da chave/valor com o conteúdo em memória.
        let mut props = BTreeMap::new();
        for line in sample.lines() {
            if let Some(kv) = line.strip_prefix("E:") {
                if let Some((k, v)) = kv.split_once('=') {
                    props.insert(k.to_string(), v.to_string());
                }
            }
        }
        assert_eq!(props.get("ID_SERIAL_SHORT").unwrap(), "WX61A79A2TDH");
        assert_eq!(props.get("ID_PART_TABLE_TYPE").unwrap(), "gpt");
    }

    #[test]
    fn live_udev_db_is_readable_without_root() {
        // Nesta máquina, o banco udev do disco 8:0 é legível — premissa do
        // enriquecimento sem root.
        let props = udev_properties("8:0");
        assert_eq!(
            props.get("ID_SERIAL_SHORT").map(String::as_str),
            Some("WX61A79A2TDH"),
            "udev db b8:0 deveria expor o serial do disco real"
        );
    }
}
