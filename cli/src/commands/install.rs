//! `sysforge install` — fluxo REAL de instalação do Windows 11 nesta máquina:
//! checklist honesto → autounattend.xml → cópia p/ pendrive Ventoy →
//! BootNext one-shot → reboot. O limite físico é dito com todas as letras:
//! após o reboot, quem executa é o instalador do Windows com nossas respostas.

use std::io::Write;
use std::path::{Path, PathBuf};

use serde_json::json;

use sysforge_core::boot::efi::read_efi_state;
use sysforge_core::error::{ErrorDomain, SysforgeError};
use sysforge_core::executor::Executor;
use sysforge_core::ipc::client::YuaClient;
use sysforge_core::ipc::protocol::{
    METHOD_BOOT_SET_NEXT, METHOD_SYSTEM_POWEROFF, METHOD_SYSTEM_REBOOT,
    METHOD_INSTALL_DISK_ARM, METHOD_INSTALL_DISK_PREPARE,
};
use sysforge_core::windows::checklist::{ItemStatus, run_checklist};
use sysforge_core::windows::media::copy_with_progress;
use sysforge_core::windows::unattend::{generate_autounattend, UnattendConfig, WINDOWS11_DOWNLOAD_URL};

use crate::commands::daemon::ensure_system_daemon;
use crate::ui::{self, paint};

pub struct InstallOpts {
    pub target: String,
    pub apply: bool,
    pub iso: Option<String>,
    pub edition: String,
    pub full_wipe: bool,
    pub reboot: bool,
    pub poweroff: bool,
    pub method: String,
}

pub fn run(json: bool, color: bool, opts: InstallOpts) -> Result<(), SysforgeError> {
    if opts.target != "windows11" {
        return Err(SysforgeError::new(
            sysforge_core::error::ErrorDomain::Dep,
            9,
            format!("Alvo `\u{200b}{}` ainda não é suportado pelo fluxo automatizado", opts.target),
        )
        .with_recommendation(
            "Hoje: `--target windows11` (checklist real, autounattend, BootNext). \
Linux (Ubuntu/Mint) e outros entram na próxima versão — a arquitetura de \
mídia/checklist já é agnóstica de SO; falta o autoboot por distro.",
        ));
    }
    let ex = Executor::default();
    let checklist = run_checklist(&ex)?;

    if json {
        println!("{}", serde_json::to_string_pretty(&json!({"ok": true, "checklist": checklist}))?);
        if !opts.apply {
            return Ok(());
        }
    } else {
        ui::banner(color);
        println!("{}", paint("Fluxo Windows 11 — sondagem real", "bold", color));
        println!();
        for item in &checklist.items {
            let tag = match item.status {
                ItemStatus::Ok => ui::tag_ok(color),
                ItemStatus::Warn => ui::tag_warn(color),
                ItemStatus::Fail => ui::tag_fail(color),
                ItemStatus::Info => ui::tag_info(color),
            };
            println!("  {tag} {} {}", paint(&item.title, "bold", color), paint("·", "dim", color));
            println!("      {}", paint(&item.detail, "dim", color));
            if let Some(h) = &item.hint {
                println!("      {} {}", paint("→", "cyan", color), h);
            }
        }
        println!();
        println!("  {} {}", paint("PRÓXIMO PASSO:", "bold", color), paint(&checklist.recommendation, "cyan", color));
        if checklist.isos.is_empty() {
            println!(
                "  {} ISO oficial: {}",
                paint("download:", "bold", color),
                paint(WINDOWS11_DOWNLOAD_URL, "magenta", color)
            );
        }
    }

    if !opts.apply {
        println!(
            "\n  {} modo verificação. Aplique quando os itens estiverem verdes: `sysforge install --apply`",
            paint("→", "cyan", color)
        );
        return Ok(());
    }

    // ---------------- APPLY (ações reais) ----------------
    println!(
        "\n{}",
        paint("APLICANDO — cada passo abaixo é executado de verdade", "bold", color)
    );

    // 0. full-wipe exige confirmação DIGITADA (apaga o disco 0 na instalação!)
    if opts.full_wipe && !json {
        println!(
            "\n  {} MODO DESTRUtivo: o autounattend vai APAGAR O DISCO 0 INTEIRO durante o",
            ui::tag_fail(color)
        );
        println!("  setup do Windows — incluindo este Linux Mint e TODOS os arquivos dele.");
        println!("  Backups feitos? (nada é recuperável depois)");
        print!("  Digite {} para confirmar: ", paint("APAGAR", "red", true));
        std::io::stdout().flush().ok();
        let mut input = String::new();
        std::io::stdin().read_line(&mut input)?;
        if input.trim() != "APAGAR" {
            println!("  {} confirmação não digitada — abortado (nada foi feito).", ui::tag_fail(color));
            return Ok(());
        }
    }

    // 1. Gerar autounattend.xml
    let cfg = UnattendConfig {
        edition: opts.edition.clone(),
        full_disk_wipe: opts.full_wipe,
        ..UnattendConfig::default()
    };
    let xml = generate_autounattend(&cfg)?;

    // Destino: raiz do pendrive Ventoy montado (o instalador do Windows acha
    // autounattend.xml em mídia removível) ou ~/Downloads com instrução.
    let ventoy_mount = checklist
        .media
        .iter()
        .find(|m| m.is_ventoy)
        .and_then(|m| m.mounted_at.clone());
    let unattend_path = match &ventoy_mount {
        Some(m) => Path::new(m).join("autounattend.xml"),
        None => std::env::var_os("HOME")
            .map(|h| PathBuf::from(h).join("Downloads").join("autounattend.xml"))
            .ok_or_else(|| SysforgeError::new(ErrorDomain::Io, 2, "HOME não definido"))?,
    };
    std::fs::write(&unattend_path, &xml)?;
    println!(
        "  {} autounattend.xml gerado em {}",
        ui::tag_ok(color),
        unattend_path.display()
    );
    if ventoy_mount.is_none() {
        println!(
            "      {} pendrive Ventoy não montado — copie este arquivo para a RAIZ do pendrive antes do boot",
            ui::tag_warn(color)
        );
    }

    // 2. Copiar ISO para o pendrive Ventoy (se ambos existirem)
    let iso: Option<(String, String)> = match opts.iso.clone() {
        Some(p) => Some((
            p.clone(),
            std::path::Path::new(&p)
                .file_name()
                .map(|n| n.to_string_lossy().to_string())
                .unwrap_or_else(|| p.clone()),
        )),
        None => checklist
            .isos
            .iter()
            .find(|i| i.looks_like_windows11)
            .map(|i| (i.path.clone(), i.name.clone())),
    };
    if let (Some(mount), Some((iso_path, iso_name))) = (&ventoy_mount, &iso) {
        let src = Path::new(iso_path);
        let dst = Path::new(mount).join(iso_name);
        let src_size = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
        let dst_size = std::fs::metadata(&dst).map(|m| m.len()).unwrap_or(0);
        if src_size > 0 && dst_size == src_size {
            println!("  {} ISO já presente no pendrive ({iso_name}) — cópia ignorada", ui::tag_ok(color));
        } else {
            println!("  {} copiando {iso_name} ({}):", ui::tag_info(color), ui::fmt_bytes(src_size));
            let mut last_pct = 0u64;
            copy_with_progress(src, &dst, |done, total| {
                if total > 0 {
                    let pct = done * 100 / total;
                    if pct >= last_pct + 5 {
                        last_pct = pct;
                        print!("\r      [{pct:>3}%] {} de {}  ", ui::fmt_bytes(done), ui::fmt_bytes(total));
                        std::io::stdout().flush().ok();
                    }
                }
            })?;
            println!("\r  {} ISO copiada — {} bytes reais{}", ui::tag_ok(color), ui::fmt_bytes(src_size), " ".repeat(30));
        }
    }

    // 3. BootNext one-shot para a entrada USB (via daemon system + polkit)
    //    GUARD: só arma se houver pendrive conectado — armar "USB" sem mídia
    //    é armadilha para o próximo boot do usuário.
    if checklist.media.is_empty() {
        println!(
            "  {} BootNext NÃO armado: nenhum pendrive conectado (arpar sem mídia seria armadilha no próximo boot)",
            ui::tag_warn(color)
        );
        println!(
            "      {} conecte o pendrive Ventoy e rode `sysforge install --apply` de novo",
            paint("→", "cyan", color)
        );
    } else {
        let state = read_efi_state(&ex)?;
        match state.usb_entry_id() {
            Some(usb_id) => {
                let sock = ensure_system_daemon(color)?;
                let mut client = YuaClient::connect(&sock)?;
                let r = client.call_interactive(METHOD_BOOT_SET_NEXT, json!({"entry_id": usb_id, "confirm": true}))?;
                println!(
                    "  {} BootNext → {} ({}) — one-shot, BootOrder intacto",
                    ui::tag_ok(color),
                    usb_id,
                    r["entry"]["name"].as_str().unwrap_or("?")
                );
                if let Some(snap) = r["snapshot_path"].as_str() {
                    println!("      {} snapshot prévio: {}", paint("·", "dim", color), snap);
                }
            }
            None => {
                println!(
                    "  {} sem entrada USB dedicada no firmware — no boot, use o menu F12/F9 e escolha o pendrive",
                    ui::tag_warn(color)
                );
            }
        }
    }

    // 3.5 MÉTODO DISCO (sem pendrive): ISO em disco + GRUB/wimboot.
    if opts.apply && opts.method != "usb" {
        let ready = sysforge_core::windows::diskboot::readiness();
        let usb_ativo = read_efi_state(&Executor::default()).ok().and_then(|s| s.usb_entry_id().map(|_| true)).unwrap_or(false);
        let use_disk = opts.method == "disk" || (opts.method == "auto" && !usb_ativo && ready.ready);
        if use_disk {
            println!("\n  {} MÉTODO DISCO — sem pendrive: ISO em disco + GRUB carrega o instalador na RAM (wimboot)", ui::tag_info(color));
            if !ready.ready {
                for b in &ready.blockers {
                    println!("  {} {}", ui::tag_fail(color), b);
                }
                return Err(SysforgeError::new(ErrorDomain::Boot, 21, "Método disco não está pronto"));
            }
            let sock = ensure_system_daemon(color)?;
            let mut client = YuaClient::connect(&sock)?;
            client.call_interactive(METHOD_INSTALL_DISK_PREPARE, json!({"confirm": true}))?;
            println!("  {} entrada GRUB criada + wimboot garantido", ui::tag_ok(color));
            client.call_interactive(METHOD_INSTALL_DISK_ARM, json!({"confirm": true}))?;
            println!("  {} próximo boot: DIRETO no instalador do Windows 11 (grub-reboot one-shot)", ui::tag_ok(color));
        }
    }

    // 4. Ação final automática: reboot ou poweroff — o firmware consome o
    // BootNext one-shot e entra DIRETO no instalador, sem intervenção.
    if opts.reboot || opts.poweroff {
        let sock = ensure_system_daemon(color)?;
        let mut client = YuaClient::connect(&sock)?;
        let acao = if opts.reboot { "REINICIAR" } else { "DESLIGAR" };
        println!(
            "\n  {} TUDO PRONTO. {} AGORA — ao voltar, a máquina entra DIRETO no instalador do Windows 11 (BootNext one-shot consumido pelo firmware).",
            ui::tag_ok(color),
            acao
        );
        if opts.poweroff {
            println!("  {} desligando: pressione o botão de energia normalmente para ligar e instalar", ui::tag_warn(color));
        }
        println!("  {} 5s para {} — Ctrl+C para abortar…", ui::tag_warn(color), acao.to_lowercase());
        for i in (1..=5).rev() {
            println!("      {i}…");
            std::thread::sleep(std::time::Duration::from_secs(1));
        }
        let method = if opts.reboot { METHOD_SYSTEM_REBOOT } else { METHOD_SYSTEM_POWEROFF };
        let _ = client.call_interactive(method, json!({"confirm": true}))?;
    } else {
        println!(
            "\n  agora escolha o desligamento automático:"
        );
        println!("      {} `sysforge install --apply --reboot`   → reinicia e ENTRA NO INSTALADOR sozinho", paint("→", "cyan", color));
        println!("      {} `sysforge install --apply --poweroff` → desliga; AO LIGAR, entra no instalador", paint("→", "cyan", color));
    }
    Ok(())
}
