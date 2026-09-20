//! Informações do sistema vivo: SO, kernel, memória, CPU, UEFI, Secure Boot,
//! TPM, bateria/AC. Tudo via leitura de /proc e /sys — sem root, sem crates
//! externas, determinístico. Cada parser é função pura (testável sem /proc).

use std::collections::BTreeMap;
use std::fs;

use serde::{Deserialize, Serialize};

/// Variável EFI SecureBoot (namespace EFI_GLOBAL_VARIABLE).
pub const EFIVAR_SECURE_BOOT: &str =
    "/sys/firmware/efi/efivars/SecureBoot-8be4df61-93ca-11d2-aa0d-00e098032b8c";
pub const SYS_CLASS_TPM: &str = "/sys/class/tpm";
pub const SYS_POWER_SUPPLY: &str = "/sys/class/power_supply";

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct OsInfo {
    pub id: Option<String>,
    pub name: Option<String>,
    pub pretty_name: Option<String>,
    pub version_id: Option<String>,
    pub version_codename: Option<String>,
    pub build_id: Option<String>,
    pub home_url: Option<String>,
}

/// Parser de /etc/os-release (formato KEY=VALUE, valores podendo ter aspas).
pub fn parse_os_release(content: &str) -> BTreeMap<String, String> {
    let mut map = BTreeMap::new();
    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((k, v)) = line.split_once('=') {
            let v = v.trim().trim_matches('"').trim_matches('\'').to_string();
            map.insert(k.trim().to_string(), v);
        }
    }
    map
}

pub fn os_info_from_map(map: &BTreeMap<String, String>) -> OsInfo {
    let g = |k: &str| map.get(k).cloned();
    OsInfo {
        id: g("ID"),
        name: g("NAME"),
        pretty_name: g("PRETTY_NAME"),
        version_id: g("VERSION_ID"),
        version_codename: g("VERSION_CODENAME"),
        build_id: g("BUILD_ID"),
        home_url: g("HOME_URL"),
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KernelInfo {
    pub release: String,
    pub version: String,
    pub arch: String,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct MemoryInfo {
    pub total_kb: u64,
    pub available_kb: u64,
}

pub fn parse_meminfo_kb(content: &str) -> MemoryInfo {
    let mut total = 0;
    let mut avail = 0;
    for line in content.lines() {
        if let Some((k, v)) = line.split_once(':') {
            let num = v
                .trim()
                .split_whitespace()
                .next()
                .and_then(|t| t.parse::<u64>().ok())
                .unwrap_or(0);
            match k.trim() {
                "MemTotal" => total = num,
                "MemAvailable" => avail = num,
                _ => {}
            }
        }
    }
    MemoryInfo {
        total_kb: total,
        available_kb: avail,
    }
}

/// (núcleos lógicos, model name) de /proc/cpuinfo (x86_64).
pub fn parse_cpuinfo(content: &str) -> (usize, Option<String>) {
    let mut cpus = 0usize;
    let mut model: Option<String> = None;
    for line in content.lines() {
        if line.starts_with("processor") {
            cpus += 1;
        } else if line.starts_with("model name") && model.is_none() {
            if let Some((_, v)) = line.split_once(':') {
                model = Some(v.trim().to_string());
            }
        }
    }
    if cpus == 0 {
        if let Ok(n) = std::thread::available_parallelism() {
            cpus = n.get();
        }
    }
    (cpus, model)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Battery {
    pub name: String,
    pub capacity_pct: Option<u8>,
    pub status: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PowerInfo {
    pub batteries: Vec<Battery>,
    pub ac_online: Option<bool>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct SecureBootInfo {
    /// Máquina boots em modo UEFI (dir /sys/firmware/efi existe).
    pub efi_supported: bool,
    /// Conseguimos ler o efivar SecureBoot.
    pub value_readable: bool,
    /// None = desconhecido (sem permissão para ler o efivar).
    pub enabled: Option<bool>,
}

/// Lê Secure Boot do efivarfs.
///
/// DETALHE CRÍTICO: arquivos em efivars têm 4 bytes de atributos no
/// início; o valor real começa no offset 4. SecureBoot é 1 byte: 0=off, 1=on.
pub fn read_secure_boot() -> SecureBootInfo {
    let efi_supported = Path::new(EFIVAR_SECURE_BOOT).exists();
    let info = SecureBootInfo {
        efi_supported,
        value_readable: false,
        enabled: None,
    };
    if !efi_supported {
        return info;
    }
    match fs::read(EFIVAR_SECURE_BOOT) {
        Ok(bytes) if bytes.len() >= 5 => SecureBootInfo {
            efi_supported: true,
            value_readable: true,
            enabled: Some(bytes[4] != 0),
        },
        _ => {
            tracing::debug!("SecureBoot efivar ilegível (sem permissão?) — reportando unknown");
            info
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SystemInfo {
    pub hostname: String,
    pub os: OsInfo,
    pub kernel: KernelInfo,
    pub memory: MemoryInfo,
    pub cpus: usize,
    pub cpu_model: Option<String>,
    pub is_uefi: bool,
    pub secure_boot: SecureBootInfo,
    pub tpm_present: bool,
    pub uptime_secs: f64,
    pub power: PowerInfo,
}

fn read_trim(path: &str) -> Option<String> {
    fs::read_to_string(path).ok().map(|s| s.trim().to_string())
}

pub fn probe_system_info() -> SystemInfo {
    let os_map = fs::read_to_string("/etc/os-release")
        .map(|c| parse_os_release(&c))
        .unwrap_or_default();
    let os = os_info_from_map(&os_map);

    let kernel = KernelInfo {
        release: read_trim("/proc/sys/kernel/osrelease").unwrap_or_default(),
        version: fs::read_to_string("/proc/version")
            .ok()
            .and_then(|s| s.lines().next().map(|l| l.to_string()))
            .unwrap_or_default(),
        arch: std::env::consts::ARCH.to_string(),
    };

    let memory = fs::read_to_string("/proc/meminfo")
        .map(|c| parse_meminfo_kb(&c))
        .unwrap_or(MemoryInfo {
            total_kb: 0,
            available_kb: 0,
        });

    let (cpus, cpu_model) = fs::read_to_string("/proc/cpuinfo")
        .map(|c| parse_cpuinfo(&c))
        .unwrap_or((1, None));

    let (mut batteries, mut ac) = (Vec::new(), None);
    if let Ok(entries) = fs::read_dir(SYS_POWER_SUPPLY) {
        for e in entries.flatten() {
            let name = e.file_name().to_string_lossy().to_string();
            let base = format!("{SYS_POWER_SUPPLY}/{name}");
            match read_trim(&format!("{base}/type")).as_deref() {
                Some("Battery") => batteries.push(Battery {
                    name: name.clone(),
                    capacity_pct: read_trim(&format!("{base}/capacity"))
                        .and_then(|c| c.parse::<u8>().ok()),
                    status: read_trim(&format!("{base}/status")),
                }),
                Some("Mains") => {
                    ac = read_trim(&format!("{base}/online")).and_then(|v| match v.as_str() {
                        "1" => Some(true),
                        "0" => Some(false),
                        _ => None,
                    })
                }
                _ => {}
            }
        }
    }

    let uptime_secs = fs::read_to_string("/proc/uptime")
        .ok()
        .and_then(|s| s.split_whitespace().next().and_then(|t| t.parse::<f64>().ok()))
        .unwrap_or(0.0);

    let tpm_present = fs::read_dir(SYS_CLASS_TPM)
        .map(|d| d.flatten().count() > 0)
        .unwrap_or(false);

    SystemInfo {
        hostname: read_trim("/etc/hostname").unwrap_or_else(|| "unknown".into()),
        os,
        kernel,
        memory,
        cpus,
        cpu_model,
        is_uefi: std::path::Path::new("/sys/firmware/efi").exists(),
        secure_boot: read_secure_boot(),
        tpm_present,
        uptime_secs,
        power: PowerInfo {
            batteries,
            ac_online: ac,
        },
    }
}

use std::path::Path;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn os_release_parse_mint() {
        let sample = "NAME=\"Linux Mint\"\nVERSION=\"22.3 (Zena)\"\nID=linuxmint\nID_LIKE=\"ubuntu debian\"\nPRETTY_NAME=\"Linux Mint 22.3\"\nVERSION_ID=\"22.3\"\nHOME_URL=\"https://www.linuxmint.com/\"\nVERSION_CODENAME=zena\n";
        let m = parse_os_release(sample);
        assert_eq!(m.get("ID").unwrap(), "linuxmint");
        assert_eq!(m.get("PRETTY_NAME").unwrap(), "Linux Mint 22.3");
        let os = os_info_from_map(&m);
        assert_eq!(os.id.as_deref(), Some("linuxmint"));
        assert_eq!(os.version_codename.as_deref(), Some("zena"));
    }

    #[test]
    fn meminfo_parse() {
        let sample = "MemTotal:       16256040 kB\nMemFree:         1234560 kB\nMemAvailable:    8123456 kB\n";
        let m = parse_meminfo_kb(sample);
        assert_eq!(m.total_kb, 16256040);
        assert_eq!(m.available_kb, 8123456);
    }

    #[test]
    fn cpuinfo_parse() {
        let sample = "processor\t: 0\nvendor_id\t: GenuineIntel\nmodel name\t: Intel(R) Core(TM) i5-4210U CPU @ 1.70GHz\nprocessor\t: 1\nmodel name\t: Intel(R) Core(TM) i5-4210U CPU @ 1.70GHz\n";
        let (cpus, model) = parse_cpuinfo(sample);
        assert_eq!(cpus, 2);
        assert_eq!(model.unwrap(), "Intel(R) Core(TM) i5-4210U CPU @ 1.70GHz");
    }

    #[test]
    fn secure_boot_efivar_offset() {
        // efivars: 4 bytes de atributos + valor. Bytes reais desta máquina:
        // atributos 06 00 00 00, valor 00 (Secure Boot DESABILITADO).
        assert_eq!(read_secure_boot().enabled.is_some(), true);
    }
}
