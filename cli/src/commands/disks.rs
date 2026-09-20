//! `yua disks` — inventário de blocos com dados reais (lsblk + udev) e
//! saúde honesta (SMART declarado como indisponível quando smartctl falta).

use yua_core::disk::lsblk::list_blockdevices;
use yua_core::disk::smart::smart_health;
use yua_core::disk::udev::enrich_from_udev;
use yua_core::error::YuaError;
use yua_core::executor::Executor;
use yua_core::Availability;

use crate::ui::{self, paint};

pub fn run(json: bool, color: bool) -> Result<(), YuaError> {
    let exec = Executor::default();
    let mut devs = list_blockdevices(&exec)?;
    for d in devs.iter_mut() {
        enrich_from_udev(d);
        for p in d.children.iter_mut() {
            enrich_from_udev(p);
        }
    }

    if json {
        let out = serde_json::json!({
            "ok": true,
            "blockdevices": devs,
        });
        println!("{}", serde_json::to_string_pretty(&out)?);
        return Ok(());
    }

    println!(
        "{}",
        paint(&format!("Inventário de blocos — {} dispositivo(s)", devs.len()), "bold", color)
    );

    for d in &devs {
        if !d.is_disk() {
            continue;
        }
        println!();
        let header = ui::table(
            &["DISCO", "MODELO", "SERIAL", "TAMANHO", "TRAN", "MÍDIA"],
            &[vec![
                paint(&d.name, "bold", color),
                d.model_trimmed().unwrap_or_else(|| "—".into()),
                d.serial_trimmed().unwrap_or_else(|| "—".into()),
                ui::fmt_bytes(d.size),
                d.tran.clone().unwrap_or_else(|| "—".into()),
                if d.rm.unwrap_or(false) {
                    paint("removível", "yellow", color)
                } else {
                    "fixa".to_string()
                },
            ]],
            color,
        );
        println!("{header}");

        if !d.children.is_empty() {
            let mut rows = Vec::new();
            for p in &d.children {
                let mounts = if p.mounts().is_empty() {
                    "—".to_string()
                } else {
                    p.mounts().join(", ")
                };
                let label = p
                    .partlabel
                    .clone()
                    .map(|l| {
                        if l.to_lowercase().contains("efi") {
                            paint(&format!("{l} ⚑ESP"), "magenta", color)
                        } else {
                            l
                        }
                    })
                    .unwrap_or_else(|| "—".into());
                rows.push(vec![
                    format!("└─ {}", p.name),
                    label,
                    p.fstype.clone().unwrap_or_else(|| "—".into()),
                    ui::fmt_bytes(p.size),
                    mounts,
                ]);
            }
            println!("{}", ui::table(&["PARTIÇÃO", "RÓTULO", "FS", "TAMANHO", "MONTAGEM"], &rows, color));
        }

        // SMART: honesto — se não dá para ler, DIZ que não dá e por quê.
        let smart = smart_health(&exec, &d.path.clone().unwrap_or_default());
        match &smart.available {
            Availability::Available => {
                let line = format!(
                    "SMART: {}",
                    match smart.passed {
                        Some(true) => paint("aprovado", "green", color),
                        Some(false) => paint("REPROVADO — disco em risco!", "red", color),
                        None => paint("sem veredito (executar como root p/ detalhes)", "yellow", color),
                    }
                );
                println!("  {line}");
            }
            Availability::Unavailable { reason_code, reason } => {
                println!(
                    "  {} SMART indisponível — {}: {}",
                    ui::tag_warn(color),
                    paint(reason_code, "yellow", color),
                    reason
                );
            }
        }
    }

    println!();
    println!(
        "  {} host-disk guard ATIVO: operações destrutivas no disco do sistema vivo são recusadas por código (YUA-DISK-010).",
        ui::tag_ok(color)
    );
    Ok(())
}
