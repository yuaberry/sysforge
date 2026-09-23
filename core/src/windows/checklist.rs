//! Checklist de instalação do Windows 11 — cada item é uma sondagem REAL
//! nesta máquina. Nada de "deve funcionar": o que falta é listado com
//! código e ação.

use std::path::PathBuf;

use serde::{Deserialize, Serialize};

use crate::boot::efi::read_efi_state;
use crate::capability::probe_capabilities;
use crate::error::SysforgeError;
use crate::executor::Executor;
use crate::hw::system::probe_system_info;
use crate::power::firmware_reboot_supported;
use crate::windows::media::list_removable_media;
use crate::windows::unattend::WINDOWS11_DOWNLOAD_URL;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ItemStatus {
    Ok,
    Warn,
    Fail,
    Info,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ChecklistItem {
    pub id: String,
    pub status: ItemStatus,
    pub title: String,
    pub detail: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub hint: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IsoFile {
    pub path: String,
    pub name: String,
    pub size_bytes: u64,
    /// true se o nome sugere Windows 11.
    pub looks_like_windows11: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct WindowsChecklist {
    pub items: Vec<ChecklistItem>,
    pub isos: Vec<IsoFile>,
    /// Mídias removíveis detectadas (com Ventoy identificado).
    pub media: Vec<crate::windows::media::RemovableMedia>,
    /// Recomendação agregada do próximo passo.
    pub recommendation: String,
}

/// Diretórios procurados por ISOs (padrões do usuário + mídia montada).
fn iso_search_dirs() -> Vec<PathBuf> {
    let mut dirs = Vec::new();
    if let Some(home) = std::env::var_os("HOME") {
        let h = PathBuf::from(home);
        for d in ["Downloads", "Downloads/windows", "Área de trabalho", "Documentos", "isos"] {
            dirs.push(h.join(d));
        }
    }
    if let Some(xdg_user) = std::env::var_os("USER") {
        let media = PathBuf::from(format!("/media/{}", xdg_user.to_string_lossy()));
        if let Ok(rd) = std::fs::read_dir(&media) {
            for e in rd.flatten() {
                dirs.push(e.path());
            }
        }
    }
    dirs
}

fn looks_like_windows11(name: &str) -> bool {
    let n = name.to_lowercase();
    n.contains("win") && (n.contains("11") || n.contains("win11")) && !n.ends_with(".zip")
}

fn find_isos() -> Vec<IsoFile> {
    let mut out: Vec<IsoFile> = Vec::new();
    for dir in iso_search_dirs() {
        let Ok(rd) = std::fs::read_dir(&dir) else { continue };
        for e in rd.flatten() {
            let p = e.path();
            let Some(name) = p.file_name().map(|n| n.to_string_lossy().to_string()) else { continue };
            if !name.to_lowercase().ends_with(".iso") {
                continue;
            }
            let size = e.metadata().map(|m| m.len()).unwrap_or(0);
            // Ignora stubs; ISO real de Win11 tem > 4 GiB.
            if size < 3 * 1024 * 1024 * 1024 {
                continue;
            }
            out.push(IsoFile {
                path: p.display().to_string(),
                name,
                size_bytes: size,
                looks_like_windows11: looks_like_windows11(&p.display().to_string()),
            });
        }
    }
    out.sort_by(|a, b| b.size_bytes.cmp(&a.size_bytes));
    out.dedup_by(|a, b| a.path == b.path);
    out
}

fn item(id: &str, status: ItemStatus, title: &str, detail: String, hint: Option<String>) -> ChecklistItem {
    ChecklistItem {
        id: id.into(),
        status,
        title: title.into(),
        detail,
        hint,
    }
}

use crate::windows::checklist::ItemStatus::{Fail, Info, Ok as OkS, Warn};

/// Sonda tudo o que importa para o fluxo Windows 11 — de verdade.
pub fn run_checklist(executor: &Executor) -> Result<WindowsChecklist, SysforgeError> {
    let sys = probe_system_info();
    let caps = probe_capabilities();
    let isos = find_isos();
    let media = list_removable_media(executor)?;
    let efi = read_efi_state(executor);

    let mut items = Vec::new();

    // 1 — UEFI
    items.push(if sys.is_uefi {
        item("uefi", OkS, "UEFI nativo", "firmware EFI presente — instalação no modo moderno".into(), None)
    } else {
        item("uefi", Fail, "UEFI nativo", "máquina não boota em UEFI".into(), Some("O fluxo automático exige UEFI.".into()))
    });

    // 2 — TPM/CPU (requisitos oficiais do Win11)
    let hw_note = format!(
        "CPU: {} · TPM: {}",
        sys.cpu_model.clone().unwrap_or_default(),
        if sys.tpm_present { "detectado" } else { "não detectado" }
    );
    items.push(item(
        "requisitos",
        if sys.tpm_present { Info } else { Warn },
        "Requisitos oficiais do Windows 11",
        hw_note,
        Some(
            "Hardware antigo é instalável com o bypass LabConfig, que o SYSFORGE inclui no autounattend. \
             Você assume a responsabilidade pelo suporte futuro da Microsoft."
            .into(),
        ),
    ));

    // 3 — Secure Boot
    items.push(match sys.secure_boot.enabled {
        Some(true) => item("secureboot", Info, "Secure Boot", "habilitado — instalação oficial flui".into(), None),
        Some(false) => item("secureboot", Info, "Secure Boot", "desabilitado — bypass incluído no unattend".into(), None),
        None => item("secureboot", Warn, "Secure Boot", "estado ilegível".into(), None),
    });

    // 4 — ISO
    let win11_isos: Vec<_> = isos.iter().filter(|i| i.looks_like_windows11).collect();
    items.push(match win11_isos.first() {
        Some(iso) => item(
            "iso",
            OkS,
            "ISO do Windows 11",
            format!("{} · {} GB", iso.name, iso.size_bytes / (1024 * 1024 * 1024)),
            Some(iso.path.clone()),
        ),
        None if !isos.is_empty() => item(
            "iso",
            Warn,
            "ISO do Windows 11",
            format!("nenhum ISO com cara de Win11; outros encontrados: {}", isos.len()),
            Some(format!("Baixe o oficial: {WINDOWS11_DOWNLOAD_URL}")),
        ),
        None => item(
            "iso",
            Fail,
            "ISO do Windows 11",
            "nenhuma ISO encontrada nos diretórios padrão".into(),
            Some(format!("Baixe o oficial em {WINDOWS11_DOWNLOAD_URL} e salve em ~/Downloads")),
        ),
    });

    // 5 — Mídia removível
    let ventoy = media.iter().find(|m| m.is_ventoy);
    items.push(match (media.first(), ventoy) {
        (Some(m), Some(_)) => item(
            "media",
            OkS,
            "Mídia USB",
            format!(
                "{} · {} · {} · Ventoy{}",
                m.name,
                m.model.clone().unwrap_or_else(|| "—".into()),
                fmt_gb(m.size_bytes),
                m.mounted_at
                    .as_ref()
                    .map(|x| format!(" montado em {x}"))
                    .unwrap_or_default()
            ),
            None,
        ),
        (Some(m), None) => item(
            "media",
            Warn,
            "Mídia USB",
            format!(
                "{} · {} presente, mas SEM Ventoy — cópia FAT32 pode falhar (install.wim > 4 GiB)",
                m.name,
                fmt_gb(m.size_bytes)
            ),
            Some("Recomendado: instale Ventoy no pendrive (você já usa Ventoy) e copie a ISO + autounattend.".into()),
        ),
        (None, _) => item(
            "media",
            Fail,
            "Mídia USB",
            "nenhum pendrive/SD detectado".into(),
            Some("Conecte um pendrive (≥ 8 GiB) — Ventoy recomendado.".into()),
        ),
    });

    // 6 — Entrada USB no UEFI (para BootNext)
    items.push(match efi {
        Ok(ref state) if state.usb_entry_id().is_some() => item(
            "usb_entry",
            OkS,
            "Entrada USB no firmware",
            format!("ID {} disponível para BootNext", state.usb_entry_id().unwrap_or("?")),
            None,
        ),
        Ok(_) => item(
            "usb_entry",
            Warn,
            "Entrada USB no firmware",
            "sem entrada USB dedicada — boot manual via menu F12/F9".into(),
            None,
        ),
        Err(e) => item("usb_entry", Warn, "Entrada USB no firmware", format!("efibootmgr falhou: {}", e.code), None),
    });

    // 7 — Reboot para firmware suportado (para "entrar na BIOS")
    items.push(if firmware_reboot_supported() {
        item("firmware_reboot", OkS, "Reboot direto na BIOS (OsIndications)", "firmware suporta BOOT_TO_FIRMWARE_UI".into(), None)
    } else {
        item("firmware_reboot", Warn, "Reboot direto na BIOS", "firmware não anuncia suporte — entrada manual (F2/Del)".into(), None)
    });

    // 8 — Ferramentas
    let has7z = caps.tools.iter().any(|t| t.name == "7z" && t.found);
    let has_rsync = caps.tools.iter().any(|t| t.name == "rsync" && t.found);
    let has_wimlib = caps.tools.iter().any(|t| t.name == "wimlib-imagex" && t.found);
    items.push(item(
        "tools",
        if has7z && has_rsync { OkS } else { Fail },
        "Ferramentas de mídia",
        format!(
            "7z: {} · rsync: {} · wimlib (split >4GiB): {}",
            yn(has7z),
            yn(has_rsync),
            yn(has_wimlib)
        ),
        Some("sudo apt install p7zip-full rsync wimtools (ou bash scripts/bootstrap-linux.sh)".into()),
    ));

    // 9 — Daemon system
    let daemon_up = crate::ipc::YuaClient::connect(std::path::Path::new(crate::ipc::DEFAULT_SYSTEM_SOCKET)).is_ok();
    items.push(if daemon_up {
        item("daemon", OkS, "Daemon sysforge-osd (system)", "privilégios prontos".into(), None)
    } else {
        item(
            "daemon",
            Warn,
            "Daemon sysforge-osd (system)",
            "não está rodando — BootNext/reboot precisam dele".into(),
            Some("Rode `sysforge daemon system` e autorize no diálogo do polkit (sua senha).".into()),
        )
    });

    let rec = build_recommendation(&items);
    Ok(WindowsChecklist { items, isos, media, recommendation: rec })
}

fn yn(b: bool) -> &'static str {
    if b { "ok" } else { "ausente" }
}

fn fmt_gb(b: u64) -> String {
    format!("{:.0} GiB", b as f64 / (1024.0 * 1024.0 * 1024.0))
}

fn build_recommendation(items: &[ChecklistItem]) -> String {
    let fails = items.iter().filter(|i| i.status == Fail).count();
    if fails > 0 {
        return "Corrija os itens vermelhos antes de prosseguir (cada um tem instruções).".into();
    }
    if items.iter().any(|i| i.id == "daemon" && i.status == Warn) {
        return "Próximo passo: `sysforge daemon system` (autorize com sua senha no polkit) e rode `sysforge install --apply`.".into();
    }
    "Tudo pronto: rode `sysforge install --apply` para gerar autounattend, armar BootNext e reiniciar.".into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn checklist_is_honest_on_real_machine() {
        // Nesta máquina: sem pendrive, sem ISO Win11 — o checklist DEVE
        // reportar exatamente isso (fail honesto), e UEFI ok.
        let ex = Executor::default();
        let c = run_checklist(&ex).unwrap();
        assert!(c.items.iter().any(|i| i.id == "uefi" && i.status == OkS), "UEFI ok");
        assert!(
            c.items.iter().any(|i| i.id == "media" && i.status == Fail),
            "sem USB conectado ⇒ fail honesto"
        );
        assert!(
            c.items.iter().any(|i| i.id == "firmware_reboot" && i.status == OkS),
            "Vostro suporta reboot→BIOS"
        );
        assert!(!c.recommendation.is_empty());
    }

    #[test]
    fn iso_pattern_matching() {
        assert!(looks_like_windows11("Win11_24H2_Brazilian_x64.iso"));
        assert!(looks_like_windows11("windows 11.iso"));
        assert!(!looks_like_windows11("linuxmint-22.3.iso"));
    }
}
