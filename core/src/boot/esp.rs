//! ESP (EFI System Partition): descoberta via /proc/mounts + espaço via
//! statvfs. Escrita na ESP é restrita (umask=0077 ⇒ root-only) — o app
//! de desktop NUNCA escreve direto; toda escrita passa pelo daemon (root),
//! no Milestone 1. Aqui: apenas leitura honesta do estado.

use std::fs;

use serde::{Deserialize, Serialize};

use crate::Availability;

pub const ESP_MOUNT_POINT: &str = "/boot/efi";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct EspInfo {
    pub mounted: bool,
    pub mount_point: Option<String>,
    pub device: Option<String>,
    pub fs_type: Option<String>,
    pub mount_options: Option<String>,
    /// true quando umask=0077/fmask=0077 ⇒ escrita só via daemon (root).
    pub restricted_permissions: bool,
    pub total_bytes: u64,
    pub free_bytes: u64,
}

impl EspInfo {
    pub fn availability(&self) -> Availability {
        if !self.mounted {
            return Availability::unavailable(
                "YUA-BOOT-002",
                "ESP (/boot/efi) não está montada",
            );
        }
        Availability::Available
    }
}

/// Uma linha de /proc/mounts: "dev  mountpoint  fstype  options  0 0".
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MountEntry {
    pub device: String,
    pub mount_point: String,
    pub fs_type: String,
    pub options: String,
}

pub fn parse_mounts_line(line: &str) -> Option<MountEntry> {
    let mut it = line.split_whitespace();
    let device = it.next()?;
    let mount_point = it.next()?;
    let fs_type = it.next()?;
    let options = it.next().unwrap_or("");
    Some(MountEntry {
        device: device.to_string(),
        mount_point: unescape_mount(mount_point),
        fs_type: fs_type.to_string(),
        options: options.to_string(),
    })
}

/// /proc/mounts escapa espaço como \040 e tab como \011.
fn unescape_mount(s: &str) -> String {
    s.replace("\\040", " ").replace("\\011", "\\t")
}

fn fs_stats(path: &str) -> Option<(u64, u64)> {
    use std::ffi::CString;
    let c = CString::new(path).ok()?;
    let mut st: libc::statvfs = unsafe { std::mem::zeroed() };
    let rc = unsafe { libc::statvfs(c.as_ptr(), &mut st) };
    if rc != 0 {
        return None;
    }
    let frsize = if st.f_frsize > 0 {
        st.f_frsize as u64
    } else {
        st.f_bsize as u64
    };
    Some((st.f_blocks as u64 * frsize, st.f_bavail as u64 * frsize))
}

/// Lê o estado REAL da ESP. Sem root, com dados reais de /proc + statvfs.
pub fn read_esp() -> EspInfo {
    let mut info = EspInfo {
        mounted: false,
        mount_point: None,
        device: None,
        fs_type: None,
        mount_options: None,
        restricted_permissions: false,
        total_bytes: 0,
        free_bytes: 0,
    };
    if let Ok(mounts) = fs::read_to_string("/proc/mounts") {
        for line in mounts.lines() {
            if let Some(entry) = parse_mounts_line(line) {
                if entry.mount_point == ESP_MOUNT_POINT {
                    let restricted = entry.options.contains("umask=0077")
                        || entry.options.contains("fmask=0077")
                        || entry.options.contains("uid=0,gid=0");
                    if let Some((total, free)) = fs_stats(&entry.mount_point) {
                        info.total_bytes = total;
                        info.free_bytes = free;
                    }
                    info.mounted = true;
                    info.mount_point = Some(entry.mount_point);
                    info.device = Some(entry.device);
                    info.fs_type = Some(entry.fs_type);
                    info.mount_options = Some(entry.options);
                    info.restricted_permissions = restricted;
                    break;
                }
            }
        }
    }
    info
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_proc_mounts_line() {
        let line = "/dev/sda1 /boot/efi vfat rw,relatime,fmask=0077,umask=0077 0 0";
        let e = parse_mounts_line(line).unwrap();
        assert_eq!(e.device, "/dev/sda1");
        assert_eq!(e.mount_point, "/boot/efi");
        assert_eq!(e.fs_type, "vfat");
        assert!(e.options.contains("umask=0077"));
    }

    #[test]
    fn live_esp_mounted_restricted_with_space() {
        // Máquina real: ESP montada em /boot/efi com umask=0077 (systemd),
        // 512 MiB, com espaço real reportado por statvfs.
        let esp = read_esp();
        assert!(esp.mounted, "ESP deveria estar montada");
        assert_eq!(esp.device.as_deref(), Some("/dev/sda1"));
        assert!(esp.restricted_permissions, "ESP com umask=0077 ⇒ restrita");
        assert!(esp.total_bytes > 400 * 1024 * 1024 && esp.total_bytes < 600 * 1024 * 1024);
        assert!(esp.free_bytes > 0);
    }
}
