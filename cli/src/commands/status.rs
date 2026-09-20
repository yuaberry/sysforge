//! `yua status` — visão geral real do sistema vivo.

use yua_core::boot::efi::read_efi_state;
use yua_core::boot::esp::read_esp;
use yua_core::executor::Executor;
use yua_core::hw::system::probe_system_info;
use yua_core::error::YuaError;

use crate::ui::{self, paint};

pub fn run(json: bool, color: bool) -> Result<(), YuaError> {
    let sys = probe_system_info();
    let esp = read_esp();
    let efi = read_efi_state(&Executor::default()).ok();

    if json {
        let out = serde_json::json!({
            "ok": true,
            "system": sys,
            "esp": esp,
            "efi": efi,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    ui::banner(color);

    let kv = |k: &str, v: String| {
        println!("  {} {}", paint(&format!("{:<12}", k), "dim", color), v);
    };

    println!("{} Sistema", ui::tag_info(color));
    kv("hostname", sys.hostname.clone());
    kv(
        "SO",
        sys.os
            .pretty_name
            .clone()
            .unwrap_or_else(|| "desconhecido".into()),
    );
    kv(
        "Kernel",
        format!("{} ({})", sys.kernel.release, sys.kernel.arch),
    );
    kv(
        "CPUs",
        format!(
            "{} × {}",
            sys.cpus,
            sys.cpu_model.clone().unwrap_or_default()
        ),
    );
    let used_kb = sys.memory.total_kb.saturating_sub(sys.memory.available_kb);
    let mem_pct = if sys.memory.total_kb > 0 {
        used_kb as f64 / sys.memory.total_kb as f64 * 100.0
    } else {
        0.0
    };
    kv(
        "Memória",
        format!(
            "{} {} em uso de {}",
            ui::bar(mem_pct, 18, color),
            ui::fmt_bytes(used_kb * 1024),
            ui::fmt_bytes(sys.memory.total_kb * 1024)
        ),
    );
    kv("Uptime", ui::fmt_uptime(sys.uptime_secs));
    println!();

    println!("{} Boot", ui::tag_info(color));
    kv(
        "Modo",
        if sys.is_uefi {
            "UEFI nativo".to_string()
        } else {
            paint("BIOS legado — recursos UEFI indisponíveis", "yellow", color)
        },
    );
    let sb = match sys.secure_boot.enabled {
        Some(true) => paint("habilitado", "yellow", color),
        Some(false) => paint("desabilitado", "green", color),
        None => paint("desconhecido (ilegível sem permissão)", "yellow", color),
    };
    kv("Secure Boot", sb);
    if esp.mounted {
        let pct_used = if esp.total_bytes > 0 {
            (esp.total_bytes - esp.free_bytes) as f64 / esp.total_bytes as f64 * 100.0
        } else {
            0.0
        };
        kv(
            "ESP",
            format!(
                "{} · {} · {} · {} livres de {}",
                paint("montada", "green", color),
                esp.device.clone().unwrap_or_default(),
                esp.fs_type.clone().unwrap_or_default(),
                ui::fmt_bytes(esp.free_bytes),
                ui::fmt_bytes(esp.total_bytes)
            ),
        );
        kv(
            "ESP espaço",
            format!("{} uso da ESP", ui::bar(pct_used, 18, color)),
        );
    } else {
        kv(
            "ESP",
            paint(
                "NÃO montada — YUA-BOOT-002 (instalação UEFI ficará indisponível)",
                "red",
                color,
            ),
        );
    }
    if let Some(state) = &efi {
        let current = state
            .boot_current
            .as_ref()
            .and_then(|id| state.entries.iter().find(|e| &e.id == id));
        if let Some(entry) = current {
            kv(
                "Boot atual",
                format!(
                    "{} {} · {}",
                    entry.id,
                    entry.name,
                    entry.loader_path.clone().unwrap_or_default()
                ),
            );
        }
    }
    println!();

    println!("{} Energia & hardware", ui::tag_info(color));
    for b in &sys.power.batteries {
        let cap = b
            .capacity_pct
            .map(|c| format!("{c}%"))
            .unwrap_or_else(|| "n/d".into());
        let status = b.status.clone().unwrap_or_default();
        kv(
            "Bateria",
            format!(
                "{} · {} {}",
                b.name,
                paint(&cap, "green", color),
                status.to_lowercase()
            ),
        );
    }
    match sys.power.ac_online {
        Some(true) => kv("Fonte AC", paint("conectada", "green", color)),
        Some(false) => kv("Fonte AC", paint("desconectada", "yellow", color)),
        None => {}
    }
    kv(
        "TPM",
        if sys.tpm_present {
            "detectado".to_string()
        } else {
            paint("não detectado", "dim", color)
        },
    );
    println!();
    Ok(())
}
