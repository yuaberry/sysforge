//! `sysforge boot` — leitura UEFI + CONTROLE REAL: BootNext one-shot,
//! reboot-direto-no-BIOS (OsIndications) e limpeza de entradas mortas.

use std::io::Write;
use std::path::PathBuf;

use serde_json::json;

use sysforge_core::boot::efi::read_efi_state;
use sysforge_core::boot::esp::read_esp;
use sysforge_core::error::{ErrorDomain, SysforgeError};
use sysforge_core::executor::Executor;
use sysforge_core::hw::system::read_secure_boot;
use sysforge_core::ipc::client::YuaClient;
use sysforge_core::ipc::protocol::{
    METHOD_BOOT_ARM_FIRMWARE, METHOD_BOOT_CLEAR_NEXT, METHOD_BOOT_REBOOT_TO_FIRMWARE,
    METHOD_BOOT_REMOVE_ENTRY, METHOD_BOOT_SET_NEXT,
};
use sysforge_core::power::firmware_reboot_supported;

use crate::commands::daemon::ensure_system_daemon;
use crate::ui::{self, paint};

use crate::BootAction;

pub fn run(json: bool, color: bool, action: Option<BootAction>) -> Result<(), SysforgeError> {
    match action {
        None => show(json, color),
        Some(BootAction::Next { entry, clear }) => next(json, color, entry, clear),
        Some(BootAction::Firmware { arm }) => firmware(json, color, arm),
        Some(BootAction::Remove { entry }) => remove(json, color, entry),
    }
}

fn show(json: bool, color: bool) -> Result<(), SysforgeError> {
    let exec = Executor::default();
    let state = read_efi_state(&exec)?;
    let esp = read_esp();
    let sb = read_secure_boot();

    if json {
        let out = serde_json::json!({
            "ok": true,
            "secure_boot": sb,
            "esp": esp,
            "efi": state,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!("{}", paint("Estado UEFI (leitura real via efibootmgr)", "bold", color));
    println!();
    let sb_txt = match sb.enabled {
        Some(true) => paint("habilitado", "yellow", color),
        Some(false) => paint("desabilitado", "green", color),
        None => paint("desconhecido", "yellow", color),
    };
    println!("  Secure Boot: {sb_txt}");
    println!(
        "  Timeout do firmware: {}",
        state.timeout_secs.map(|t| format!("{t}s")).unwrap_or_else(|| "—".into())
    );
    if let Some(bn) = &state.boot_next {
        println!(
          "  BootNext (one-shot armado): {} {}",
          paint("▶", "magenta", color),
          paint(bn, "magenta", color)
        );
    } else {
        println!("  BootNext: nenhum");
    }
    println!(
        "  BootOrder: {}",
        if state.boot_order.is_empty() { "—".to_string() } else { state.boot_order.join(" → ") }
    );
    println!();

    let mut rows = Vec::new();
    for e in &state.entries {
        let marker = if Some(&e.id) == state.boot_current.as_ref() {
            paint("▶ AQUI", "magenta", color)
        } else {
            String::new()
        };
        let mut name = e.name.clone();
        if e.is_dead_file_entry() {
            name = format!("{name} {}", paint("☠ MORTA", "red", color));
        } else if e.is_firmware_internal() {
            name = format!("{name} {}", paint("(firmware)", "dim", color));
        }
        rows.push(vec![
            marker,
            paint(&e.id, "bold", color),
            if e.active { paint("sim", "green", color) } else { paint("não", "dim", color) },
            name,
            e.loader_path.clone().unwrap_or_else(|| e.device_path.clone().unwrap_or_else(|| "—".into())),
        ]);
    }
    println!(
        "{}",
        ui::table(&["BOOT ATUAL", "ID", "ATIVO", "NOME", "LOADER / DEVICE PATH"], &rows, color)
    );

    println!();
    println!(
        "  {} controle: `sysforge boot next <ID>` · `sysforge boot firmware` (reinicia na BIOS) · `sysforge boot remove <ID>`",
        ui::tag_info(color)
    );
    Ok(())
}

fn connect_system(color: bool) -> Result<(PathBuf, YuaClient), SysforgeError> {
    let sock = ensure_system_daemon(color)?;
    let client = YuaClient::connect(&sock)?;
    Ok((sock, client))
}

fn next(json: bool, color: bool, entry: Option<String>, clear: bool) -> Result<(), SysforgeError> {
    if clear {
        let (_, mut client) = connect_system(color)?;
        let r = client.call_interactive(METHOD_BOOT_CLEAR_NEXT, json!({"confirm": true}))?;
        if json {
            println!("{}", serde_json::to_string_pretty(&r)?);
        } else if r["cleared"].as_bool().unwrap_or(true) {
            println!("  {} BootNext cancelado — o boot seguirá o BootOrder normal", ui::tag_ok(color));
        } else {
            println!("  {} Nada estava armado — BootNext já estava limpo", ui::tag_ok(color));
        }
        return Ok(());
    }

    // Auto-detecção da entrada USB quando o ID não vem explícito.
    let entry_id = match entry {
        Some(id) => id,
        None => {
            let state = read_efi_state(&Executor::default())?;
            state
                .usb_entry_id()
                .map(|s| s.to_string())
                .ok_or_else(|| {
                    SysforgeError::new(
                        ErrorDomain::Boot,
                        6,
                        "Nenhuma entrada USB encontrada para auto-detectar",
                    )
                    .with_recommendation("Passe o ID explícito (`sysforge boot` lista os IDs) ou conecte o pendrive e tente de novo.")
                })?
        }
    };

    let (_, mut client) = connect_system(color)?;
    let r = client.call_interactive(METHOD_BOOT_SET_NEXT, json!({"entry_id": entry_id, "confirm": true}))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&r)?);
        return Ok(());
    }
    println!(
        "  {} BootNext armado: {} ({})",
        ui::tag_ok(color),
        r["boot_next"].as_str().unwrap_or(&entry_id),
        r["entry"]["name"].as_str().unwrap_or("?")
    );
    println!("      {} one-shot: consumido no próximo boot, BootOrder INTACTO", paint("·", "dim", color));
    if let Some(snap) = r["snapshot_path"].as_str() {
        println!("      {} snapshot prévio: {}", paint("·", "dim", color), snap);
    }
    println!(
        "      {} reiniciar agora: `sysforge power reboot --confirm`",
        paint("→", "cyan", color)
    );
    Ok(())
}

fn firmware(json: bool, color: bool, arm_only: bool) -> Result<(), SysforgeError> {
    if !firmware_reboot_supported() {
        return Err(SysforgeError::new(
            ErrorDomain::Uefi,
            1,
            "Este firmware não suporta reboot-direto-no-setup",
        )
        .with_recommendation("Entre no setup manualmente no boot (F2/Del)."));
    }
    let (_, mut client) = connect_system(color)?;
    if arm_only {
        let r = client.call_interactive(METHOD_BOOT_ARM_FIRMWARE, json!({"confirm": true}))?;
        if json {
            println!("{}", serde_json::to_string_pretty(&r)?);
        } else {
            println!(
                "  {} armado: o PRÓXIMO boot abre a tela da BIOS (sem reiniciar agora)",
                ui::tag_ok(color)
            );
            println!("      {} qualquer reboot/finalização consome o pedido", paint("·", "dim", color));
        }
        return Ok(());
    }

    println!(
        "  {} o computador vai REINICIAR AGORA direto na tela do BIOS/UEFI setup",
        ui::tag_warn(color)
    );
    println!("  {} 3s para abortar com Ctrl+C…", paint("·", "dim", color));
    for i in (1..=3).rev() {
        println!("      {i}…");
        std::thread::sleep(std::time::Duration::from_secs(1));
    }
    let _ = std::io::stdout().flush();
    let r = client.call_interactive(METHOD_BOOT_REBOOT_TO_FIRMWARE, json!({"confirm": true}))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&r)?);
    } else {
        println!("  {} reiniciando para o setup do firmware…", ui::tag_ok(color));
    }
    Ok(())
}

fn remove(json: bool, color: bool, entry: String) -> Result<(), SysforgeError> {
    // Mostra o que será removido ANTES de pedir confirmação digitada.
    let state = read_efi_state(&Executor::default())?;
    let Some(e) = state.entry(&entry) else {
        return Err(SysforgeError::new(
            ErrorDomain::Boot,
            3,
            format!("Entrada de boot {entry} não existe"),
        )
        .with_recommendation("`sysforge boot` lista os IDs válidos."));
    };
    if e.is_firmware_internal() {
        return Err(SysforgeError::new(
            ErrorDomain::Boot,
            5,
            format!("{} é entrada INTERNA do firmware — intocável", e.name),
        ));
    }
    println!("  {} entrada a remover: {} {} {}", ui::tag_warn(color), paint(&e.id, "bold", color), e.name, if e.is_dead_file_entry() { paint("(MORTA — loader vazio)", "red", color) } else { String::new() });
    if let Some(dp) = &e.device_path {
        println!("      {}", paint(dp, "dim", color));
    }
    print!("  Digite o ID {} para confirmar a remoção: ", paint(&e.id, "bold", true));
    std::io::stdout().flush().ok();
    let mut input = String::new();
    std::io::stdin().read_line(&mut input)?;
    if input.trim() != e.id {
        println!("  {} confirmação não digitada — nada removido.", ui::tag_fail(color));
        return Ok(());
    }

    let (_, mut client) = connect_system(color)?;
    let r = client.call_interactive(METHOD_BOOT_REMOVE_ENTRY, json!({"entry_id": entry, "confirm": true}))?;
    if json {
        println!("{}", serde_json::to_string_pretty(&r)?);
        return Ok(());
    }
    println!(
        "  {} entrada {} removida (snapshot prévio: {})",
        ui::tag_ok(color),
        r["removed"],
        r["snapshot_path"].as_str().unwrap_or("?")
    );
    Ok(())
}
