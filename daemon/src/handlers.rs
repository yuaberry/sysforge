//! Despacho de métodos v1 — read-only direto do sysforge-core; privilegiados
//! com TODAS as barreiras: snapshot prévio, confirmação explícita e guardas
//! de entrada de boot.

use serde_json::json;

use sysforge_core::boot::efi::read_efi_state;
use sysforge_core::boot::snapshot::BootSnapshot;
use sysforge_core::capability::probe_capabilities;
use sysforge_core::disk::lsblk::list_blockdevices;
use sysforge_core::disk::udev::enrich_from_udev;
use sysforge_core::error::{ErrorDomain, SysforgeError};
use sysforge_core::executor::{CommandSpec, Executor};
use sysforge_core::hw::system::probe_system_info;
use sysforge_core::ipc::protocol::{
    Request, Response, WireError, METHOD_BOOT_ARM_FIRMWARE, METHOD_BOOT_CLEAR_NEXT,
    METHOD_BOOT_REBOOT_TO_FIRMWARE, METHOD_BOOT_REMOVE_ENTRY, METHOD_BOOT_SET_NEXT,
    METHOD_BOOT_SNAPSHOT, METHOD_CAPABILITIES, METHOD_DAEMON_INFO, METHOD_DAEMON_SHUTDOWN,
    METHOD_DISKS_LIST, METHOD_ECHO, METHOD_EFI_ENTRIES, METHOD_SYSTEM_INFO, METHOD_SYSTEM_POWEROFF,
    METHOD_INSTALL_DISK_READINESS, METHOD_INSTALL_DISK_PREPARE, METHOD_INSTALL_DISK_ARM, METHOD_INSTALL_DISK_REVERT,
    METHOD_INSTALL_SMALL_USB_BUILD, METHOD_INSTALL_SMALL_USB_PLAN,
    METHOD_SYSTEM_REBOOT, PROTOCOL_VERSION,
};
use sysforge_core::power;

use crate::server::Peer;
use crate::{Config, DaemonMode, VERSION};

pub fn dispatch(req: &Request, cfg: &Config, peer: &Peer) -> Response {
    match handle(req, cfg, peer) {
        Ok(value) => Response::ok(req.id, value),
        Err(e) => Response::err(req.id, WireError::from(e)),
    }
}

/// Confirmação explícita OBRIGATÓRIA em todo método de efeito real.
/// Nada de "confirm" implícito — a UI sempre manda a decisão do usuário.
fn require_confirm(req: &Request) -> Result<(), SysforgeError> {
    if req.params.get("confirm").and_then(|c| c.as_bool()) == Some(true) {
        Ok(())
    } else {
        Err(SysforgeError::new(
            ErrorDomain::Auth,
            6,
            "Confirmação explícita ausente — operação NÃO executada",
        )
        .with_recommendation("Isto protege contra cliques acidentais. Repita com confirm:true (a UI sempre pergunta antes)."))
    }
}

fn require_str(req: &Request, key: &str) -> Result<String, SysforgeError> {
    req.params
        .get(key)
        .and_then(|v| v.as_str())
        .map(|s| s.to_string())
        .ok_or_else(|| {
            SysforgeError::new(
                ErrorDomain::Io,
                12,
                format!("Parâmetro obrigatório ausente: {key}"),
            )
        })
}

fn handle(req: &Request, cfg: &Config, _peer: &Peer) -> Result<serde_json::Value, SysforgeError> {
    let exec = Executor::default();
    match req.method.as_str() {
        // ---------- read-only ----------
        METHOD_ECHO => Ok(json!({ "echo": req.params.get("message").cloned().unwrap_or(json!("pong")) })),
        METHOD_SYSTEM_INFO => serde_json::to_value(probe_system_info()).map_err(SysforgeError::from),
        METHOD_DISKS_LIST => {
            let mut devs = list_blockdevices(&exec)?;
            for d in devs.iter_mut() {
                enrich_from_udev(d);
                for p in d.children.iter_mut() {
                    enrich_from_udev(p);
                }
            }
            serde_json::to_value(devs).map_err(SysforgeError::from)
        }
        METHOD_EFI_ENTRIES => serde_json::to_value(read_efi_state(&exec)?).map_err(SysforgeError::from),
        METHOD_CAPABILITIES => serde_json::to_value(probe_capabilities()).map_err(SysforgeError::from),
        METHOD_DAEMON_INFO => Ok(json!({
            "daemon": "sysforge-osd",
            "version": VERSION,
            "protocol": PROTOCOL_VERSION,
            "mode": cfg.mode.label(),
            "socket": cfg.socket.display().to_string(),
            "pid": std::process::id(),
            "uid_running": unsafe { libc::getuid() },
            "system_ready": cfg.mode == DaemonMode::System,
        })),
        // Snapshot é read-only (efibootmgr -v) — disponível até em dev.
        METHOD_BOOT_SNAPSHOT => {
            let snap = BootSnapshot::capture(&exec)?;
            let path = snap.save()?;
            Ok(json!({ "snapshot_path": path.display().to_string(), "state": snap.state }))
        }

        // ---------- privilegiados (system + polkit; auth.rs já validou) ----------
        METHOD_BOOT_SET_NEXT => {
            require_confirm(req)?;
            let entry_id = require_str(req, "entry_id")?;
            // Validações ANTES de tocar no firmware:
            let state = read_efi_state(&exec)?;
            let Some(entry) = state.entry(&entry_id) else {
                return Err(SysforgeError::new(
                    ErrorDomain::Boot,
                    3,
                    format!("Entrada de boot {entry_id} não existe"),
                )
                .with_recommendation("Rode `sysforge boot` para ver os IDs válidos."));
            };
            if !entry.active {
                return Err(SysforgeError::new(
                    ErrorDomain::Boot,
                    4,
                    format!("Entrada {entry_id} ({}) está INATIVA no firmware", entry.name),
                )
                .with_recommendation("Ative a entrada primeiro ou escolha outra."));
            }
            // Foto completa antes de qualquer mutação.
            let snap = BootSnapshot::capture(&exec)?;
            let snapshot_path = snap.save()?;
            // BootNext one-shot: o firmware consome e apaga no próximo boot.
            // BootOrder PERMANECE INTACTO por construção (nunca passamos -o).
            let spec = CommandSpec::new("efibootmgr")
                .arg("--bootnext")
                .arg(entry_id)
                .timeout(std::time::Duration::from_secs(20));
            let r = exec.run_low_risk(spec)?;
            if !r.success() {
                return Err(SysforgeError::command_failed("efibootmgr --bootnext", &[], r.exit_code, &r.stderr));
            }
            let now = read_efi_state(&exec)?;
            Ok(json!({
                "boot_next": now.boot_next,
                "entry": { "id": entry.id, "name": entry.name },
                "one_shot": true,
                "boot_order_untouched": true,
                "snapshot_path": snapshot_path.display().to_string(),
            }))
        }

        METHOD_BOOT_CLEAR_NEXT => {
            require_confirm(req)?;
            let snap = BootSnapshot::capture(&exec)?;
            snap.save()?;
            // Nada armado? BootNext limpo por natureza — não executa o
            // --delete-bootnext (efibootmgr status 17 = "não existe" é ruído, não erro).
            if snap.state.boot_next.is_none() {
                return Ok(json!({ "cleared": false, "already_clean": true }));
            }
            let spec = CommandSpec::new("efibootmgr")
                .arg("--delete-bootnext")
                .timeout(std::time::Duration::from_secs(20));
            let r = exec.run_low_risk(spec)?;
            if !r.success() {
                // Quirk real de campo (efibootmgr 2.39.3): com BootNext=0000 o
                // --delete-bootnext falha com 17 ("Boot entry 0001 does not
                // exist") DEIXANDO a variável no firmware. Fallback
                // determinístico: unlink direto na efivarfs (o kernel deleta a
                // variável UEFI efetivamente).
                const BOOTNEXT_VAR: &str =
                    "/sys/firmware/efi/efivars/BootNext-8be4df61-93ca-11d2-aa0d-00e098032b8c";
                if std::path::Path::new(BOOTNEXT_VAR).exists() {
                    std::fs::remove_file(BOOTNEXT_VAR)?;
                    tracing::warn!(
                        exit = ?r.exit_code,
                        "efibootmgr --delete-bootnext falhou; BootNext removido via efivarfs (quirk valor 0000)"
                    );
                }
            }
            // Verificação de verdade: releitura do estado após a limpeza.
            let after = BootSnapshot::capture(&exec)?;
            Ok(json!({ "cleared": after.state.boot_next.is_none() }))
        }

        // ── Método DISCO: instalar sem pendrive (ISO em disco + GRUB/wimboot) ──
        METHOD_INSTALL_DISK_READINESS => {
            Ok(serde_json::to_value(sysforge_core::windows::diskboot::readiness())?)
        }
        METHOD_INSTALL_DISK_PREPARE => {
            require_confirm(req)?;
            sysforge_core::windows::diskboot::prepare(&exec)
        }
        METHOD_INSTALL_DISK_ARM => {
            require_confirm(req)?;
            sysforge_core::windows::diskboot::arm_grub_next_boot(&exec)?;
            Ok(json!({ "armed": true, "via": "grub-reboot (one-shot)" }))
        }
        METHOD_INSTALL_DISK_REVERT => {
            require_confirm(req)?;
            sysforge_core::windows::diskboot::revert(&exec)?;
            Ok(json!({ "reverted": true }))
        }

        // ── Pendrive otimizado (4GB): mídia de boot DIRETA, 100% padrão Microsoft ──
        METHOD_INSTALL_SMALL_USB_PLAN => {
            let dev = require_str(req, "device")?;
            let edition: u32 = req.params.get("edition_index").and_then(|v| v.as_u64()).unwrap_or(4) as u32;
            sysforge_core::windows::smallusb::plan(&dev, edition).map(|p| serde_json::to_value(p).unwrap_or(json!(null)))
        }
        METHOD_INSTALL_SMALL_USB_BUILD => {
            require_confirm(req)?;
            let dev = require_str(req, "device")?;
            let edition: u32 = req.params.get("edition_index").and_then(|v| v.as_u64()).unwrap_or(4) as u32;
            let unattend = req.params.get("autounattend").and_then(|v| v.as_str()).map(std::path::PathBuf::from);
            let identity = sysforge_core::disk::DiskIdentity::probe(&exec, &dev)?;
            sysforge_core::windows::smallusb::build(&exec, &identity, edition, unattend.as_deref())
        }

        METHOD_BOOT_REMOVE_ENTRY => {
            require_confirm(req)?;
            let entry_id = require_str(req, "entry_id")?;
            let state = read_efi_state(&exec)?;
            let Some(entry) = state.entry(&entry_id) else {
                return Err(SysforgeError::new(
                    ErrorDomain::Boot,
                    3,
                    format!("Entrada de boot {entry_id} não existe"),
                ));
            };
            // Guardas: internas do firmware são INTOCÁVEIS.
            if entry.is_firmware_internal() {
                return Err(SysforgeError::new(
                    ErrorDomain::Boot,
                    5,
                    format!("Entrada {} é INTERNA do firmware — recusado", entry.name),
                )
                .with_recommendation("Setup/Diagnostics/menus são gerados pelo próprio firmware; removê-las pode quebrar o boot."));
            }
            if !entry.is_removable_by_os() {
                return Err(SysforgeError::new(
                    ErrorDomain::Boot,
                    5,
                    format!("Entrada {} não é uma entrada de disco removível pelo SO", entry.name),
                ));
            }
            let snap = BootSnapshot::capture(&exec)?;
            let snapshot_path = snap.save()?;
            let spec = CommandSpec::new("efibootmgr")
                .arg("-b").arg(entry_id.clone())
                .arg("-B")
                .timeout(std::time::Duration::from_secs(20));
            let r = exec.run_low_risk(spec)?;
            if !r.success() {
                return Err(SysforgeError::command_failed("efibootmgr -B", &[], r.exit_code, &r.stderr));
            }
            Ok(json!({
                "removed": entry_id,
                "was": { "id": entry.id, "name": entry.name, "device_path": entry.device_path },
                "snapshot_path": snapshot_path.display().to_string(),
            }))
        }

        METHOD_BOOT_ARM_FIRMWARE => {
            require_confirm(req)?;
            let snap = BootSnapshot::capture(&exec)?;
            let _ = snap.save();
            power::arm_firmware_reboot()?;
            Ok(json!({
                "armed": true,
                "detail": "próximo boot abre a tela de setup do firmware (BIOS)",
            }))
        }

        METHOD_BOOT_REBOOT_TO_FIRMWARE => {
            require_confirm(req)?;
            power::reboot_to_firmware(&exec)?;
            Ok(json!({ "rebooting_to_firmware": true }))
        }

        METHOD_SYSTEM_REBOOT => {
            require_confirm(req)?;
            let r = power::system_reboot(&exec)?;
            Ok(json!({ "rebooting": r.success() }))
        }

        METHOD_SYSTEM_POWEROFF => {
            require_confirm(req)?;
            let r = power::system_poweroff(&exec)?;
            Ok(json!({ "powering_off": r.success() }))
        }

        // Encerramento limpo: responde ANTES de sair (o cliente precisa do OK).
        METHOD_DAEMON_SHUTDOWN => {
            require_confirm(req)?;
            tracing::info!("shutdown solicitado via IPC — encerrando em 300ms");
            std::thread::spawn(|| {
                std::thread::sleep(std::time::Duration::from_millis(300));
                std::process::exit(0);
            });
            Ok(json!({ "shutting_down": true }))
        }

        other => {
            let e = SysforgeError::new(
                ErrorDomain::NotSupported,
                1,
                format!("Método desconhecido: {other}"),
            )
            .with_technical("protocolo v1: echo, system.info, disks.list, efi.entries, capabilities, daemon.info, boot.snapshot + (system) boot.set_next/clear_next/remove_entry/arm_firmware/reboot_to_firmware, system.reboot/poweroff");
            Err(e)
        }
    }
}
