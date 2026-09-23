//! `sysforge logs` — últimas linhas do log do SYSFORGE + onde ficam os de auditoria.

use std::fs;

use sysforge_core::error::{ErrorDomain, SysforgeError};
use sysforge_core::logging;

use crate::ui::paint;

pub fn run(json: bool, color: bool, lines: usize) -> Result<(), SysforgeError> {
    let Some(path) = logging::default_app_log_file() else {
        return Err(SysforgeError::new(ErrorDomain::Io, 2, "HOME não definido"));
    };
    let audit_dev = std::env::var_os("HOME")
        .map(|h| format!("{}/.local/share/sysforge/logs/audit.jsonl", h.to_string_lossy()))
        .unwrap_or_default();

    if !path.exists() {
        let msg = format!("Nenhum log ainda — será criado em {}", path.display());
        if json {
            println!(
                "{}",
                serde_json::json!({"ok": true, "exists": false, "path": path.display().to_string()})
            );
        } else {
            println!("{msg}");
            println!("Auditoria (daemon dev):   {audit_dev}");
            println!("Auditoria (daemon system): /var/lib/sysforge/audit.jsonl");
        }
        return Ok(());
    }

    let content = fs::read_to_string(&path)?;
    let all: Vec<&str> = content.lines().collect();
    let start = all.len().saturating_sub(lines.max(1));

    if json {
        println!(
            "{}",
            serde_json::json!({
                "ok": true,
                "path": path.display().to_string(),
                "total_lines": all.len(),
                "lines": &all[start..],
            })
        );
        return Ok(());
    }

    println!(
        "{}",
        paint(
            &format!("=== {} (últimas {} de {} linhas) ===", path.display(), all.len() - start, all.len()),
            "bold",
            color
        )
    );
    for l in &all[start..] {
        println!("{l}");
    }
    println!();
    println!("{}", paint("Auditoria do daemon:", "dim", color));
    println!("  dev:    {audit_dev}");
    println!("  system: /var/lib/sysforge/audit.jsonl");
    Ok(())
}
