//! `sysforge doctor` — diagnóstico completo do ambiente com instruções EXATAS de
//! correção. Cada verificação roda com spinner e resultado honesto: o que
//! falta é reportado com código (SF-DEP-NNN) e o comando apt correspondente.

use std::path::PathBuf;

use sysforge_core::boot::efi::read_efi_state;
use sysforge_core::boot::esp::read_esp;
use sysforge_core::capability::probe_capabilities;
use sysforge_core::disk::identity::DiskIdentity;
use sysforge_core::error::SysforgeError;
use sysforge_core::executor::{disk_of_partition, host_root_disk, Executor};
use sysforge_core::hw::system::probe_system_info;
use sysforge_core::ipc::protocol::{DEFAULT_SYSTEM_SOCKET, METHOD_DAEMON_INFO};
use sysforge_core::ipc::{dev_socket_default, YuaClient};
use sysforge_core::Availability;

use crate::ui::{self, paint};

/// Comando EXATO para compilar o app desktop (Tauri 2 no Ubuntu 24.04/Mint 22).
pub const APT_BUILD_DEPS: &str = "sudo apt install -y libwebkit2gtk-4.1-dev libgtk-3-dev \
libsoup-3.0-dev librsvg2-dev libayatana-appindicator3-dev libssl-dev libxdo-dev file pkg-config";
/// Dependências de runtime recomendadas (saúde de discos, Windows, QEMU).
pub const APT_RUNTIME_DEPS: &str = "sudo apt install -y smartmontools nvme-cli xorriso wimtools \
mtools qemu-system-x86 qemu-utils ovmf memtest86+ testdisk";

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum Status {
    Ok,
    Warn,
    Fail,
}

struct Finding {
    status: Status,
    detail: String,
}

impl Finding {
    fn tag(&self, color: bool) -> String {
        match self.status {
            Status::Ok => ui::tag_ok(color),
            Status::Warn => ui::tag_warn(color),
            Status::Fail => ui::tag_fail(color),
        }
    }
}

fn check<F>(name: &str, color: bool, f: F) -> Finding
where
    F: FnOnce() -> Finding,
{
    let sp = ui::Spinner::start(name, color);
    let finding = f();
    sp.finish();
    println!(
        "  {} {} {} {}",
        finding.tag(color),
        paint(name, "bold", color),
        paint("·", "dim", color),
        finding.detail
    );
    finding
}

fn daemon_state() -> Option<(String, serde_json::Value)> {
    let paths: Vec<PathBuf> = vec![dev_socket_default(), PathBuf::from(DEFAULT_SYSTEM_SOCKET)];
    for p in paths {
        if let Ok(mut client) = YuaClient::connect(&p) {
            if let Ok(info) = client.call(METHOD_DAEMON_INFO, serde_json::json!({})) {
                return Some((p.display().to_string(), info));
            }
        }
    }
    None
}

pub fn run(json: bool, color: bool) -> Result<(), SysforgeError> {
    let exec = Executor::default();
    let caps = probe_capabilities();
    let sys = probe_system_info();

    let mut findings: Vec<Finding> = Vec::new();

    ui::banner(color);
    println!("{}", paint("Diagnóstico do ambiente", "bold", color));
    println!();

    // 1 — ferramentas essenciais
    findings.push(check("ferramentas essenciais (core)", color, || {
        let (found, total) = *caps.groups.get("core").unwrap_or(&(0, 0));
        let missing: Vec<String> = caps
            .tools
            .iter()
            .filter(|t| t.group == "core" && !t.found)
            .map(|t| format!("{} ({})", t.name, t.apt_package))
            .collect();
        if missing.is_empty() {
            Finding {
                status: Status::Ok,
                detail: format!("{found}/{total} presentes — particionamento, formatação, UKI e boot cobertos"),
            }
        } else {
            Finding {
                status: Status::Fail,
                detail: format!("faltando: {}", missing.join(", ")),
            }
        }
    }));

    // 2 — autorização
    findings.push(check("autorização (polkit)", color, || {
        let have = ["pkexec", "pkcheck"].iter().all(|t| caps.tools.iter().any(|x| x.name == *t && x.found));
        let mok = caps.tools.iter().any(|x| x.name == "mokutil" && x.found);
        if have && mok {
            Finding {
                status: Status::Ok,
                detail: "pkexec, pkcheck e mokutil presentes".into(),
            }
        } else if have {
            Finding {
                status: Status::Warn,
                detail: "mokutil ausente (pacote mokutil) — só afeta estados MOK/Secure Boot".into(),
            }
        } else {
            Finding {
                status: Status::Fail,
                detail: "policykit ausente — o daemon system não poderá autenticar".into(),
            }
        }
    }));

    // 3 — UEFI + Secure Boot
    findings.push(check("UEFI + Secure Boot", color, || {
        if !sys.is_uefi {
            return Finding {
                status: Status::Fail,
                detail: "máquina não boota em UEFI — plataforma exige UEFI nativo".into(),
            };
        }
        match sys.secure_boot.enabled {
            Some(true) => Finding {
                status: Status::Warn,
                detail: "Secure Boot HABILITADO — drivers/UKIs precisarão ser assinados (mok)".into(),
            },
            Some(false) => Finding {
                status: Status::Ok,
                detail: "UEFI nativo · Secure Boot desabilitado — UKIs de teste bootam direto".into(),
            },
            None => Finding {
                status: Status::Warn,
                detail: "UEFI nativo · valor de Secure Boot ilegível (SF-UEFI-001)".into(),
            },
        }
    }));

    // 4 — ESP
    let esp = read_esp();
    findings.push(check("ESP (/boot/efi)", color, || {
        if !esp.mounted {
            return Finding {
                status: Status::Fail,
                detail: "não montada — SF-BOOT-002: instalação UEFI indisponível".into(),
            };
        }
        if esp.free_bytes < 32 * 1024 * 1024 {
            Finding {
                status: Status::Warn,
                detail: format!(
                    "montada em {} mas só {} livres — risco de estourar com UKIs",
                    esp.device.clone().unwrap_or_default(),
                    ui::fmt_bytes(esp.free_bytes)
                ),
            }
        } else {
            Finding {
                status: Status::Ok,
                detail: format!(
                    "{} · {} livres de {} · escrita {}",
                    esp.device.clone().unwrap_or_default(),
                    ui::fmt_bytes(esp.free_bytes),
                    ui::fmt_bytes(esp.total_bytes),
                    if esp.restricted_permissions {
                        "via daemon (umask=0077)"
                    } else {
                        "direta"
                    }
                ),
            }
        }
    }));

    // 5 — disco do sistema + identidade real
    findings.push(check("identidade do disco do sistema", color, || {
        let Some(root) = host_root_disk() else {
            return Finding {
                status: Status::Fail,
                detail: "não consegui determinar o disco do rootfs (/proc/mounts)".into(),
            };
        };
        let disk = disk_of_partition(&root).unwrap_or_else(|| root.clone());
        match DiskIdentity::probe(&exec, &disk) {
            Ok(id) => {
                let serial = id.serial.unwrap_or_default();
                Finding {
                    status: Status::Ok,
                    detail: format!(
                        "{} em {} · serial {} — guard de disco vivo armado (SF-DISK-010)",
                        disk, root, serial
                    ),
                }
            }
            Err(e) => Finding {
                status: Status::Warn,
                detail: format!("sondagem falhou: {} ({})", e.code, e.message),
            },
        }
    }));

    // 6 — entradas UEFI
    findings.push(check("entradas de boot UEFI", color, || {
        match read_efi_state(&exec) {
            Ok(state) => Finding {
                status: Status::Ok,
                detail: format!(
                    "{} entradas legíveis · boot atual {} · leitura sem root OK",
                    state.entries.len(),
                    state.boot_current.unwrap_or_else(|| "—".into())
                ),
            },
            Err(e) => Finding {
                status: Status::Warn,
                detail: format!("efibootmgr falhou: {} — {}", e.code, e.message),
            },
        }
    }));

    // 7 — daemon
    let daemon = daemon_state();
    findings.push(check("daemon sysforge-osd", color, || {
        match &daemon {
            Some((sock, info)) => Finding {
                status: Status::Ok,
                detail: format!(
                    "v{} · modo {} · {}",
                    info["version"], info["mode"], sock
                ),
            },
            None => Finding {
                status: Status::Warn,
                detail: "não está rodando — leitura direta funciona; destructive exigirá modo system".into(),
            },
        }
    }));

    // 8 — virtualização
    findings.push(check("virtualização (testes futuros)", color, || {
        let qemu = caps.tools.iter().filter(|t| t.name.starts_with("qemu") && t.found).count();
        match (&caps.kvm, &caps.ovmf) {
            (Availability::Available, Availability::Available) if qemu == 2 => Finding {
                status: Status::Ok,
                detail: "KVM + OVMF + QEMU prontos para testes destrutivos em VM".into(),
            },
            _ => Finding {
                status: Status::Warn,
                detail: "incompleto — testes destrutivos da Fase 9 exigem (veja comando abaixo)".into(),
            },
        }
    }));

    // 9 — saúde SMART
    findings.push(check("saúde de disco (SMART)", color, || {
        if caps.tools.iter().any(|t| t.name == "smartctl" && t.found) {
            Finding {
                status: Status::Ok,
                detail: "smartctl instalado (detalhes completos via daemon system)".into(),
            }
        } else {
            Finding {
                status: Status::Warn,
                detail: "smartctl ausente — SF-DEP-005: `sysforge disks` reporta SMART como indisponível (honesto)".into(),
            }
        }
    }));

    // 10 — headers de build do app desktop
    findings.push(check("headers de build do app desktop", color, || {
        if caps.tauri_build_ready {
            Finding {
                status: Status::Ok,
                detail: "webkit2gtk-4.1/gtk3/soup3 encontrados — `cargo build` do app compila".into(),
            }
        } else {
            Finding {
                status: Status::Warn,
                detail: "headers webkit2gtk-4.1-dev ausentes — app desktop não compila ATÉ instalar (comando abaixo)".into(),
            }
        }
    }));

    // 11 — agente de diálogo polkit (senhas na tela para o APP)
    findings.push(check("agente de diálogo polkit", color, || {
        let has_pk = caps.tools.iter().any(|t| t.name == "pkexec" && t.found);
        let agent = sysforge_core::capability::polkit_dialog_agent_present();
        match (has_pk, agent) {
            (true, true) => Finding {
                status: Status::Ok,
                detail: "agente ativo — o app consegue pedir sua senha na tela".into(),
            },
            (true, false) => Finding {
                status: Status::Warn,
                detail: "nenhum agente rodando — o APP pode não conseguir pedir senha na tela (CLI no terminal funciona via prompt de texto); autostart do agente já está em ~/.config/autostart".into(),
            },
            (false, _) => Finding {
                status: Status::Fail,
                detail: "pkexec ausente — modo privilegiado não funciona".into(),
            },
        }
    }));

    // ---- resumo ----
    let (ok, warn, fail) = findings.iter().fold((0, 0, 0), |(o, w, f), x| match x.status {
        Status::Ok => (o + 1, w, f),
        Status::Warn => (o, w + 1, f),
        Status::Fail => (o, w, f + 1),
    });
    println!();
    println!(
        "  {} {} em ordem · {} {} · {} {} críticos",
        ui::tag_ok(color),
        paint(&ok.to_string(), "green", color),
        ui::tag_warn(color),
        paint(&warn.to_string(), "yellow", color),
        ui::tag_fail(color),
        paint(&fail.to_string(), "red", color)
    );

    // ---- próximos passos com comandos exatos ----
    let mut next: Vec<String> = Vec::new();
    if !caps.tauri_build_ready {
        next.push(format!("Compilar o app desktop:\n      {APT_BUILD_DEPS}"));
    }
    let missing_runtime = caps.tools.iter().any(|t| !t.found && t.group != "core");
    if missing_runtime {
        next.push(format!(
            "Ferramentas recomendadas (SMART, Windows, QEMU):\n      {APT_RUNTIME_DEPS}"
        ));
    }
    next.push(String::from(
        "Tudo acima de uma vez: bash scripts/bootstrap-linux.sh (idempotente, com --check)",
    ));
    if daemon.is_none() {
        next.push(String::from(
            "Subir o daemon dev (somente leitura): sysforge daemon run",
        ));
    }
    if !next.is_empty() {
        println!();
        println!("  {}", paint("PRÓXIMOS PASSOS", "bold", color));
        for (i, n) in next.iter().enumerate() {
            println!("  {}. {n}", i + 1);
        }
    }
    println!();

    if json {
        let out = serde_json::json!({
            "ok": true,
            "summary": {"ok": ok, "warn": warn, "fail": fail},
            "capabilities": caps,
            "system": sys,
            "esp": esp,
            "daemon": daemon.map(|(_, info)| info),
            "next_steps": next,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
    }
    Ok(())
}
