//! `yua boot` — estado UEFI real: entradas, BootOrder, Secure Boot, ESP.

use yua_core::boot::efi::read_efi_state;
use yua_core::boot::esp::read_esp;
use yua_core::error::YuaError;
use yua_core::executor::Executor;
use yua_core::hw::system::read_secure_boot;

use crate::ui::{self, paint};

pub fn run(json: bool, color: bool) -> Result<(), YuaError> {
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
        state
            .timeout_secs
            .map(|t| format!("{t}s"))
            .unwrap_or_else(|| "—".into())
    );
    println!(
        "  BootOrder: {}",
        if state.boot_order.is_empty() {
            "—".to_string()
        } else {
            state.boot_order.join(" → ")
        }
    );
    println!();

    let mut rows = Vec::new();
    for e in &state.entries {
        let marker = if Some(&e.id) == state.boot_current.as_ref() {
            paint("▶ AQUI", "magenta", color)
        } else {
            String::new()
        };
        rows.push(vec![
            marker,
            paint(&e.id, "bold", color),
            if e.active {
                paint("sim", "green", color)
            } else {
                paint("não", "dim", color)
            },
            e.name.clone(),
            e.loader_path
                .clone()
                .unwrap_or_else(|| e.device_path.clone().unwrap_or_else(|| "—".into())),
        ]);
    }
    println!(
        "{}",
        ui::table(&["BOOT ATUAL", "ID", "ATIVO", "NOME", "LOADER / DEVICE PATH"], &rows, color)
    );

    println!();
    if esp.mounted {
        let pct_used = if esp.total_bytes > 0 {
            (esp.total_bytes - esp.free_bytes) as f64 / esp.total_bytes as f64 * 100.0
        } else {
            0.0
        };
        println!(
            "  {} ESP: {} {} · {} livres de {} {}",
            ui::tag_ok(color),
            esp.device.clone().unwrap_or_default(),
            paint(
                if esp.restricted_permissions {
                    "(escrita restrita: root-only — via daemon)"
                } else {
                    ""
                },
                "dim",
                color
            ),
            ui::fmt_bytes(esp.free_bytes),
            ui::fmt_bytes(esp.total_bytes),
            ui::bar(pct_used, 12, color)
        );
    } else {
        println!("  {} ESP não montada (YUA-BOOT-002)", ui::tag_fail(color));
    }

    println!();
    println!(
        "  {} Modo leitura — escrita EFI (BootNext one-shot) chega no Milestone 1; nunca alteramos BootOrder permanente.",
        ui::tag_info(color)
    );
    Ok(())
}
