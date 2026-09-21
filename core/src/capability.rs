//! Probe de capacidades do ambiente: ferramentas externas, KVM, OVMF,
//! headers de build do Tauri. O doctor (`yua doctor`) consome isto para
//! dizer com precisão o que está pronto e o que falta instalar — sem
//! adivinhação.

use std::collections::BTreeMap;
use std::path::PathBuf;

use serde::Serialize;

use crate::executor::{which, CommandSpec, Executor};
use crate::Availability;

/// Especificação estática de uma ferramenta externa.
pub struct ToolSpec {
    pub name: &'static str,
    pub group: &'static str,
    pub purpose: &'static str,
    /// Pacote apt que instala a ferramenta.
    pub apt_package: &'static str,
}

/// Registro completo de ferramentas usadas pelo YUA.
/// Grupos: core (essencial), auth, health, windows, virtual.
pub const TOOL_SPECS: &[ToolSpec] = &[
    // ---- essenciais (presentes no Mint 22.3 por padrão) ----
    ToolSpec { name: "lsblk", group: "core", purpose: "inventário de blocos (disco/partição)", apt_package: "util-linux" },
    ToolSpec { name: "blkid", group: "core", purpose: "identificação de filesystem", apt_package: "util-linux" },
    ToolSpec { name: "findmnt", group: "core", purpose: "estado de montagens", apt_package: "util-linux" },
    ToolSpec { name: "efibootmgr", group: "core", purpose: "leituras/entradas de boot UEFI", apt_package: "efibootmgr" },
    ToolSpec { name: "sgdisk", group: "core", purpose: "particionamento GPT (sgdisk)", apt_package: "gdisk" },
    ToolSpec { name: "sfdisk", group: "core", purpose: "particionamento scriptável", apt_package: "util-linux" },
    ToolSpec { name: "parted", group: "core", purpose: "particionamento interativo/scrippable", apt_package: "parted" },
    ToolSpec { name: "wipefs", group: "core", purpose: "limpeza de assinaturas de disco", apt_package: "util-linux" },
    ToolSpec { name: "mkfs.vfat", group: "core", purpose: "formatação FAT32 (ESP)", apt_package: "dosfstools" },
    ToolSpec { name: "mkfs.ext4", group: "core", purpose: "formatação ext4 (Linux)", apt_package: "e2fsprogs" },
    ToolSpec { name: "mkfs.ntfs", group: "core", purpose: "formatação NTFS (Windows)", apt_package: "ntfs-3g" },
    ToolSpec { name: "ntfsfix", group: "core", purpose: "reparo leve de NTFS", apt_package: "ntfs-3g" },
    ToolSpec { name: "btrfs", group: "core", purpose: "utilitário Btrfs", apt_package: "btrfs-progs" },
    ToolSpec { name: "rsync", group: "core", purpose: "cópia de árvores de arquivos (staging)", apt_package: "rsync" },
    ToolSpec { name: "7z", group: "core", purpose: "extração de imagens ISO/WIM", apt_package: "p7zip-full" },
    ToolSpec { name: "jq", group: "core", purpose: "debug/validação de JSON", apt_package: "jq" },
    ToolSpec { name: "objcopy", group: "core", purpose: "montagem de UKI (kernel+initrd+cmdline)", apt_package: "binutils" },
    ToolSpec { name: "file", group: "core", purpose: "identificação de arquivos", apt_package: "file" },
    ToolSpec { name: "curl", group: "core", purpose: "download de imagens", apt_package: "curl" },
    ToolSpec { name: "wget", group: "core", purpose: "download de imagens (alternativa)", apt_package: "wget" },
    // ---- autorização ----
    ToolSpec { name: "pkexec", group: "auth", purpose: "execução privilegiada via polkit", apt_package: "policykit-1" },
    ToolSpec { name: "pkcheck", group: "auth", purpose: "verificação de autorização polkit do chamador", apt_package: "policykit-1" },
    ToolSpec { name: "mokutil", group: "auth", purpose: "estado de Secure Boot/MOK", apt_package: "mokutil" },
    // ---- saúde de hardware (ausentes no Mint por padrão) ----
    ToolSpec { name: "smartctl", group: "health", purpose: "diagnóstico SMART de discos", apt_package: "smartmontools" },
    ToolSpec { name: "nvme", group: "health", purpose: "diagnóstico NVMe", apt_package: "nvme-cli" },
    ToolSpec { name: "testdisk", group: "health", purpose: "recuperação de partições", apt_package: "testdisk" },
    // ---- Windows ----
    ToolSpec { name: "wimlib-imagex", group: "windows", purpose: "aplicação de WIM/ESD (install.wim)", apt_package: "wimtools" },
    ToolSpec { name: "mcopy", group: "windows", purpose: "cópia de arquivos em FAT sem montar (mtools)", apt_package: "mtools" },
    // ---- virtualização (testes destrutivos seguros) ----
    ToolSpec { name: "qemu-system-x86_64", group: "virtual", purpose: "VM de testes de instalação real", apt_package: "qemu-system-x86" },
    ToolSpec { name: "qemu-img", group: "virtual", purpose: "criação de discos virtuais de teste", apt_package: "qemu-utils" },
];

#[derive(Debug, Clone, Serialize)]
pub struct ToolStatus {
    pub name: String,
    pub group: &'static str,
    pub purpose: &'static str,
    pub apt_package: &'static str,
    pub found: bool,
    pub path: Option<String>,
}

/// Varre o PATH (sem shell — implementação Rust pura) e reporta cada tool.
pub fn probe_tools() -> Vec<ToolStatus> {
    TOOL_SPECS
        .iter()
        .map(|t| {
            let found = which(t.name);
            ToolStatus {
                name: t.name.to_string(),
                group: t.group,
                purpose: t.purpose,
                apt_package: t.apt_package,
                found: found.is_some(),
                path: found.map(|p: PathBuf| p.to_string_lossy().to_string()),
            }
        })
        .collect()
}

/// Verifica um módulo pkg-config sem depender do shell.
fn pkg_config_exists(module: &str) -> bool {
    if which("pkg-config").is_none() {
        return false;
    }
    let exec = Executor::default();
    let spec = CommandSpec::new("pkg-config").arg("--exists").arg(module);
    matches!(exec.run_readonly(spec), Ok(r) if r.success())
}

/// Existe um AGENTE DE DIÁLOGO do polkit rodando na sessão?
/// Sem agente, o app desktop (sem TTY) não consegue mostrar o diálogo de
/// senha — só funciona via terminal (prompt de texto). CLI tem fallback.
///
/// Lê /proc/*/cmdline (completo — `comm` trunca em 15 chars) e casa
/// "authentication-agent" (MATE/GNOME/KDE). O `polkit-agent-helper` NÃO casa
/// (é o verificador root spawned durante auth, não um agente de diálogo).
pub fn polkit_dialog_agent_present() -> bool {
    let Ok(dirs) = std::fs::read_dir("/proc") else {
        return false;
    };
    for entry in dirs.flatten() {
        let Ok(cmdline) =
            std::fs::read_to_string(format!("/proc/{}/cmdline", entry.file_name().to_string_lossy()))
        else {
            continue;
        };
        // cmdline separa args com \0; normaliza para busca simples.
        let full = cmdline.replace('\0', " ");
        if full.contains("authentication-agent") {
            return true;
        }
    }
    false
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityReport {
    pub tools: Vec<ToolStatus>,
    /// Resumo por grupo: group -> (presentes, total).
    pub groups: BTreeMap<String, (usize, usize)>,
    pub kvm: Availability,
    pub ovmf: Availability,
    pub memtest86: Availability,
    /// true quando os headers de build do Tauri (webkit2gtk-4.1, gtk+-3.0,
    /// libsoup-3.0) estão instalados — pré-requisito para compilar o app.
    pub tauri_build_ready: bool,
}

pub fn probe_capabilities() -> CapabilityReport {
    let tools = probe_tools();
    let mut groups: BTreeMap<String, (usize, usize)> = BTreeMap::new();
    for t in &tools {
        let e = groups.entry(t.group.to_string()).or_insert((0, 0));
        e.1 += 1;
        if t.found {
            e.0 += 1;
        }
    }

    let kvm = if std::path::Path::new("/dev/kvm").exists() {
        Availability::Available
    } else {
        Availability::unavailable(
            "YUA-DEP-001",
            "/dev/kvm ausente — testes em VM ficarão lentos (emulação) ou indisponíveis",
        )
    };

    let ovmf = ["/usr/share/OVMF/OVMF_CODE_4M.fd", "/usr/share/OVMF/OVMF_CODE.fd", "/usr/share/ovmf/OVMF.fd"]
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(|_| Availability::Available)
        .unwrap_or_else(|| {
            Availability::unavailable("YUA-DEP-002", "firmware UEFI OVMF não encontrado (pacote ovmf)")
        });

    let memtest86 = ["/boot/memtest86+x64.efi", "/boot/efi/memtest86+/memtest86+x64.efi"]
        .iter()
        .find(|p| std::path::Path::new(p).exists())
        .map(|_| Availability::Available)
        .unwrap_or_else(|| {
            Availability::unavailable("YUA-DEP-003", "memtest86+ não instalado (pacote memtest86+)")
        });

    let tauri_build_ready = [
        "webkit2gtk-4.1",
        "gtk+-3.0",
        "libsoup-3.0",
        "javascriptcoregtk-4.1",
    ]
    .iter()
    .all(|m| pkg_config_exists(m));

    CapabilityReport {
        tools,
        groups,
        kvm,
        ovmf,
        memtest86,
        tauri_build_ready,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn probe_tools_reflects_reality() {
        let tools = probe_tools();
        // Ferramentas essenciais confirmadas presentes no Mint 22.3 desta máquina.
        assert!(tools.iter().any(|t| t.name == "lsblk" && t.found));
        assert!(tools.iter().any(|t| t.name == "efibootmgr" && t.found));
        // smartctl é sabidamente ausente aqui — o probe deve dizer a verdade.
        let smart = tools.iter().find(|t| t.name == "smartctl").unwrap();
        assert!(!smart.found, "smartctl não deveria estar presente nesta máquina");
    }

    #[test]
    fn capability_report_groups() {
        let rep = probe_capabilities();
        let core = rep.groups.get("core").expect("grupo core sempre existe");
        assert!(core.0 >= 10, "no Mint 22.3 as ferramentas essenciais vêm por padrão");
    }
}
