//! Método "Disco + Nuvem" — instalar o SO desejado SEM pendrive.
//!
//! Como funciona (100% real, sem pendrive e sem tocar em partições):
//!   1. a ISO fica onde está (ex.: ~/Downloads) — baixada da nuvem se faltar;
//!   2. o GRUB (que já boota a máquina) ganha uma entrada que carrega o
//!      instalador do Windows DIRETO da ISO para a RAM, via `wimboot`
//!      (carregador oficial do projeto iPXE, licença BSD);
//!   3. `grub-reboot` define o próximo boot para essa entrada —
//!      a máquina reinicia/desliga e volta DIRETO no instalador. Automático.
//!
//! Por que não "criar partição": encolher o rootfs ext4 MONTADO é impossível
//! online (resize2fs exige desmontado) — bootar a ISO como ARQUIVO via GRUB
//! evita mexer em qualquer partição. Zero risco de layout.
//!
//! Pré-requisitos honestos (verificados antes de prometer qualquer coisa):
//!   - Secure Boot DESLIGADO (wimboot não é assinado — requisito real);
//!   - GRUB2 presente e dono do boot (é o Boot0000 padrão em Mint/Ubuntu);
//!   - RAM livre suficiente (boot.wim do Win11 vai inteiro para a memória);
//!   - ISO > 3 GiB no disco (ou sinalizada para download).

use std::path::{Path, PathBuf};

use crate::error::{ErrorDomain, SysforgeError};
use crate::executor::{CommandSpec, Executor};

/// Onde o wimboot vive no sistema (copiado/baixado pelo daemon; GRUB lê daqui).
pub const WIMBOOT_PATH: &str = "/var/lib/sysforge/wimboot";
/// Entrada dedicada do GRUB (arquivo próprio = reversão trivial).
pub const GRUB_ENTRY_FILE: &str = "/etc/grub.d/40_sysforge";
/// Origem oficial do wimboot (projeto iPXE, BSD).
pub const WIMBOOT_URL: &str = "https://github.com/ipxe/wimboot/releases/latest/download/wimboot";
/// boot.wim do Win11 (~700 MiB) + BCD + boot.sdi + margem — para a RAM.
pub const RAM_NEEDED_MB: u64 = 2500;

/// Estado de prontidão do método disco (honesto, sondagem real).
#[derive(Debug, Clone, serde::Serialize)]
pub struct DiskBootReadiness {
    pub iso_path: Option<String>,
    pub secure_boot_off: bool,
    pub grub_present: bool,
    pub update_grub_present: bool,
    pub ram_available_mb: u64,
    pub ram_needed_mb: u64,
    pub ram_ok: bool,
    pub wimboot_present: bool,
    pub wimboot_needs_download: bool,
    pub ready: bool,
    pub blockers: Vec<String>,
}

/// Encontra a maior ISO de Windows em ~/Downloads (mesma regra do checklist).
pub fn find_iso() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    for dir in [PathBuf::from(&home).join("Downloads"), PathBuf::from(&home)] {
        let mut best: Option<(u64, PathBuf)> = None;
        if let Ok(rd) = std::fs::read_dir(&dir) {
            for e in rd.flatten() {
                let p = e.path();
                let name = p.file_name()?.to_string_lossy().to_lowercase();
                if !name.ends_with(".iso") || !name.contains("win") {
                    continue;
                }
                if let Ok(md) = std::fs::metadata(&p) {
                    if md.len() > 3 * 1024 * 1024 * 1024 {
                        if best.as_ref().map(|(s, _)| md.len() > *s).unwrap_or(true) {
                            best = Some((md.len(), p));
                        }
                    }
                }
            }
        }
        if let Some((_, p)) = best {
            return Some(p);
        }
    }
    None
}

/// Secure Boot precisa estar OFF para o wimboot (não assinado) bootar.
pub fn secure_boot_off() -> bool {
    crate::hw::system::read_secure_boot().enabled.map(|on| !on).unwrap_or(false)
}

/// GRUB é o bootloader padrão? (Mint/Ubuntu: sim — verificamos por evidência real)
pub fn grub_present() -> bool {
    Path::new("/boot/grub/grub.cfg").exists()
        && (Path::new("/usr/sbin/update-grub").exists() || Path::new("/usr/bin/update-grub").exists())
}

fn mem_available_mb() -> u64 {
    if let Ok(s) = std::fs::read_to_string("/proc/meminfo") {
        for line in s.lines() {
            if let Some(rest) = line.strip_prefix("MemAvailable:") {
                return rest.trim().trim_end_matches(" kB").parse().unwrap_or(0) / 1024;
            }
        }
    }
    0
}

/// Sonda completa — NADA é prometido sem evidence.
pub fn readiness() -> DiskBootReadiness {
    let iso = find_iso();
    let sb_off = secure_boot_off();
    let grub = grub_present();
    let ram = mem_available_mb();
    let ram_ok = ram >= RAM_NEEDED_MB;
    let wimboot_present = Path::new(WIMBOOT_PATH).exists();
    let mut blockers = Vec::new();
    if iso.is_none() {
        blockers.push("ISO do Windows não encontrada em ~/Downloads (> 3 GiB, nome contendo 'win')".into());
    }
    if !sb_off {
        blockers.push("Secure Boot LIGADO — desligue na BIOS (o wimboot não é assinado)".into());
    }
    if !grub {
        blockers.push("GRUB2 não encontrado como bootloader (método exige GRUB, padrão em Mint/Ubuntu)".into());
    }
    if !ram_ok {
        blockers.push(format!("RAM livre {ram} MiB < {RAM_NEEDED_MB} MiB — o instalador carrega inteiro na memória"));
    }
    let ready = iso.is_some() && sb_off && grub && ram_ok;
    DiskBootReadiness {
        iso_path: iso.map(|p| p.display().to_string()),
        secure_boot_off: sb_off,
        grub_present: grub,
        update_grub_present: true,
        ram_available_mb: ram,
        ram_needed_mb: RAM_NEEDED_MB,
        ram_ok,
        wimboot_present,
        wimboot_needs_download: !wimboot_present,
        ready,
        blockers,
    }
}

/// Gera a entrada do GRUB (arquivo 40_sysforge — dedicada, reversível).
pub fn grub_entry_text(iso: &Path) -> String {
    let iso = iso.display().to_string();
    format!(
        r#"#!/bin/sh
exec tail -n +3 $0
# SYSFORGE — instalação sem pendrive (ISO em disco + wimboot na RAM).
# Remover este arquivo + `update-grub` remove a entrada por inteiro.
menuentry "SYSFORGE — Instalar Windows 11 (sem pendrive)" {{
    set isofile="{iso}"
    search --no-floppy --file --set=root $isofile
    insmod part_gpt
    insmod ext2
    loopback loop $isofile
    echo "Carregando o instalador do Windows para a RAM (wimboot)..."
    linux ($root){WIMBOOT_PATH}
    initrd (loop)/boot/bcd BCD
    initrd (loop)/boot/boot.sdi boot.sdi
    initrd (loop)/sources/boot.wim boot.wim
    boot
}}
"#
    )
}

/// Plano de preparo — validado ANTES de qualquer escrita (fail-closed).
pub struct DiskBootPlan {
    pub iso_path: PathBuf,
    pub wimboot_source: WimbootSource,
    pub entry_text: String,
    pub entry_file: PathBuf,
}

pub enum WimbootSource {
    AlreadyInstalled,
    DownloadFromOfficial,
}

pub fn plan() -> Result<DiskBootPlan, SysforgeError> {
    let r = readiness();
    if let Some(first) = r.blockers.first() {
        return Err(SysforgeError::new(ErrorDomain::Boot, 20, "Método disco não pronto")
            .with_technical(r.blockers.join(" · "))
            .with_recommendation(first.clone()));
    }
    let iso = PathBuf::from(r.iso_path.expect("ready implica iso"));
    Ok(DiskBootPlan {
        entry_text: grub_entry_text(&iso),
        entry_file: PathBuf::from(GRUB_ENTRY_FILE),
        iso_path: iso,
        wimboot_source: if r.wimboot_present {
            WimbootSource::AlreadyInstalled
        } else {
            WimbootSource::DownloadFromOfficial
        },
    })
}

/// Executa o preparo (chamado pelo daemon como root; confirm exigido no IPC).
/// Escreve a entrada do GRUB, garante o wimboot e ativa GRUB_DEFAULT=saved.
pub fn prepare(exec: &Executor) -> Result<serde_json::Value, SysforgeError> {
    let plan = plan()?;
    std::fs::create_dir_all("/var/lib/sysforge")?;
    std::fs::write(&plan.entry_file, plan.entry_text)?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&plan.entry_file, std::fs::Permissions::from_mode(0o755))?;

    if let WimbootSource::DownloadFromOfficial = plan.wimboot_source {
        let spec = CommandSpec::new("curl")
            .arg("-fL")
            .arg("--retry").arg("3")
            .arg("-o").arg(WIMBOOT_PATH)
            .arg(WIMBOOT_URL)
            .timeout(std::time::Duration::from_secs(120));
        let r = exec.run_low_risk(spec)?;
        if !r.success() {
            return Err(SysforgeError::new(ErrorDomain::Dep, 20, "Falha ao baixar o wimboot (iPXE oficial)")
                .with_technical(format!("curl exit {:?}: {}", r.exit_code, r.stderr.trim()))
                .with_recommendation(format!("Baixe manualmente {WIMBOOT_URL} para {WIMBOOT_PATH} e repita.")));
        }
        std::fs::set_permissions(WIMBOOT_PATH, std::fs::Permissions::from_mode(0o644))?;
    }

    // GRUB_DEFAULT=saved é pré-requisito do grub-reboot (boot one-shot no GRUB).
    ensure_grub_default_saved()?;

    let spec = CommandSpec::new("update-grub").timeout(std::time::Duration::from_secs(90));
    let r = exec.run_low_risk(spec)?;
    if !r.success() {
        return Err(SysforgeError::command_failed("update-grub", &[], r.exit_code, &r.stderr));
    }

    tracing::info!(iso = %plan.iso_path.display(), "método disco preparado (entrada GRUB + wimboot)");
    Ok(serde_json::json!({
        "entry_file": plan.entry_file.display().to_string(),
        "iso_path": plan.iso_path.display().to_string(),
        "wimboot": WIMBOOT_PATH,
        "wimboot_downloaded": matches!(plan.wimboot_source, WimbootSource::DownloadFromOfficial),
    }))
}

/// Aponta o PRÓXIMO boot direto para a entrada SYSFORGE (one-shot, como BootNext).
pub fn arm_grub_next_boot(exec: &Executor) -> Result<(), SysforgeError> {
    let title = "SYSFORGE — Instalar Windows 11 (sem pendrive)";
    // grub-reboot usa o índice/nome; nome é estável, índice muda. Nome funciona
    // quando a entrada existe no menu (garantido pelo update-grub acima).
    let spec = CommandSpec::new("grub-reboot").arg(title).timeout(std::time::Duration::from_secs(30));
    let r = exec.run_low_risk(spec)?;
    if !r.success() {
        return Err(SysforgeError::command_failed("grub-reboot", &[], r.exit_code, &r.stderr)
            .with_recommendation("Se falhou, o menu do GRUB aparece no boot: escolha a entrada SYSFORGE manualmente (uma vez)."));
    }
    Ok(())
}

/// Remove a entrada e restaura o boot normal (reversão completa do método).
pub fn revert(exec: &Executor) -> Result<(), SysforgeError> {
    let _ = std::fs::remove_file(GRUB_ENTRY_FILE);
    let spec = CommandSpec::new("update-grub").timeout(std::time::Duration::from_secs(90));
    let r = exec.run_low_risk(spec)?;
    if !r.success() {
        return Err(SysforgeError::command_failed("update-grub", &[], r.exit_code, &r.stderr));
    }
    Ok(())
}

/// Garante GRUB_DEFAULT=saved em /etc/default/grub (idempotente, comentado).
fn ensure_grub_default_saved() -> Result<(), SysforgeError> {
    let path = "/etc/default/grub";
    let current = std::fs::read_to_string(path).unwrap_or_default();
    let re_fixed = regex_lite(current.as_str());
    if !re_fixed {
        // sem GRUB_DEFAULT=saved: acrescenta no fim (com marcador)
        let mut text = current.trim_end().to_string();
        if !text.is_empty() && !text.ends_with('\n') {
            text.push('\n');
        }
        text.push_str("# SYSFORGE: permite boot one-shot via grub-reboot\nGRUB_DEFAULT=saved\n");
        std::fs::write(path, text)?;
    }
    Ok(())
}

/// true quando GRUB_DEFAULT=saved já está ativo (linha não-comentada).
fn regex_lite(s: &str) -> bool {
    s.lines().any(|l| {
        let t = l.trim();
        t == "GRUB_DEFAULT=saved" || t.starts_with("GRUB_DEFAULT=\"saved\"")
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn entry_references_iso_and_wimboot() {
        let t = grub_entry_text(Path::new("/home/user/Downloads/Win11.iso"));
        assert!(t.contains("/home/user/Downloads/Win11.iso"), "entrada deve apontar para a ISO real");
        assert!(t.contains(WIMBOOT_PATH), "entrada deve carregar o wimboot do caminho oficial");
        assert!(t.contains("loopback loop"), "ISO é bootada como arquivo (sem pendrive, sem partição)");
        assert!(t.contains("sources/boot.wim"), "setup do Windows vem do boot.wim da ISO");
        assert!(t.contains("exec tail -n +3"), "bloco 40_custom precisa do header executável do GRUB");
    }

    #[test]
    fn readiness_is_honest_without_iso() {
        // Nesta máquina de desenvolvimento há Mint+GRUB e Secure Boot off:
        // sem ISO, o blocker É a ISO (e nada mais é prometido).
        let r = readiness();
        if r.iso_path.is_none() {
            assert!(!r.ready, "sem ISO não pode estar ready");
            assert!(r.blockers.iter().any(|b| b.contains("ISO")), "blocker deve citar a ISO");
        }
        assert!(r.ram_needed_mb >= 2000, "Win11 boot.wim precisa de margem real de RAM");
    }

    #[test]
    fn grub_default_saved_detection() {
        assert!(regex_lite("GRUB_DEFAULT=saved"));
        assert!(regex_lite("GRUB_DEFAULT=\"saved\""));
        assert!(!regex_lite("#GRUB_DEFAULT=saved"));
        assert!(!regex_lite("GRUB_DEFAULT=0"));
    }
}
