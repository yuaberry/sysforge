//! Efeitos visuais do terminal: gradiente ANSI, spinner braille, tabelas
//! box-drawing, barras de progresso e badges — com fallback limpo para
//! pipe/NO_COLOR (`color=false` emite texto puro).

use std::io::Write;
use std::io::IsTerminal;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

use colored::Colorize;

pub fn paint(s: &str, color: &str, on: bool) -> String {
    if !on {
        return s.to_string();
    }
    match color {
        "red" => s.red().to_string(),
        "green" => s.green().to_string(),
        "yellow" => s.yellow().to_string(),
        "blue" => s.blue().to_string(),
        "magenta" => s.magenta().to_string(),
        "cyan" => s.cyan().to_string(),
        "bold" => s.bold().to_string(),
        "dim" => s.dimmed().to_string(),
        _ => s.to_string(),
    }
}

/// Gradiente truecolor violeta→ciano, caractere a caractere.
pub fn gradient_line(text: &str, on: bool) -> String {
    if !on {
        return text.to_string();
    }
    let start = (156u8, 81u8, 255u8);
    let end = (0u8, 229u8, 255u8);
    let chars: Vec<char> = text.chars().collect();
    let mut out = String::new();
    for (i, c) in chars.iter().enumerate() {
        let t = if chars.len() <= 1 {
            0.0
        } else {
            i as f64 / (chars.len() - 1) as f64
        };
        let mix = |a: u8, b: u8| -> u8 { (a as f64 + (b as f64 - a as f64) * t).round() as u8 };
        out.push_str(&format!(
            "\x1b[38;2;{};{};{}m{}\x1b[0m",
            mix(start.0, end.0),
            mix(start.1, end.1),
            mix(start.2, end.2),
            c
        ));
    }
    out
}

/// Banner de abertura do `sysforge`.
pub fn banner(on: bool) {
    let rule = "━".repeat(52);
    println!("{}", paint(&rule, "magenta", on));
    println!();
    println!("  {}", gradient_line("SYSFORGE", on));
    println!(
        "  {}",
        paint(
            &format!(
                "v{} · deployment & recovery · protocolo IPC v1 · Linux",
                sysforge_core::YUA_VERSION
            ),
            "dim",
            on
        )
    );
    println!();
    println!("{}", paint(&rule, "magenta", on));
}

// ---- badges ----

pub fn tag_ok(on: bool) -> String {
    paint("✔", "green", on)
}
pub fn tag_warn(on: bool) -> String {
    paint("⚠", "yellow", on)
}
pub fn tag_fail(on: bool) -> String {
    paint("✖", "red", on)
}
pub fn tag_info(on: bool) -> String {
    paint("●", "cyan", on)
}

// ---- spinner (stderr; só anima se stderr for tty) ----

pub struct Spinner {
    stop: Arc<AtomicBool>,
    handle: Option<std::thread::JoinHandle<()>>,
}

impl Spinner {
    pub fn start(msg: &str, _color: bool) -> Self {
        let stop = Arc::new(AtomicBool::new(false));
        let stop2 = stop.clone();
        let msg = msg.to_string();
        let animate = std::io::stderr().is_terminal();
        let handle = std::thread::spawn(move || {
            let frames = ['⠋', '⠙', '⠹', '⠸', '⠼', '⠴', '⠦', '⠧', '⠇', '⠏'];
            let mut i = 0usize;
            while !stop2.load(Ordering::Relaxed) {
                if animate {
                    eprint!("\r\x1b[35m{}\x1b[0m {msg}  ", frames[i % frames.len()]);
                    let _ = std::io::stderr().flush();
                }
                i += 1;
                std::thread::sleep(std::time::Duration::from_millis(80));
            }
        });
        Self {
            stop,
            handle: Some(handle),
        }
    }

    /// Para a animação e limpa a linha do spinner (resultado vai no stdout).
    pub fn finish(mut self) {
        self.stop.store(true, Ordering::Relaxed);
        if let Some(h) = self.handle.take() {
            let _ = h.join();
        }
        if std::io::stderr().is_terminal() {
            eprint!("\r\x1b[2K");
            let _ = std::io::stderr().flush();
        }
    }
}

// ---- barras e formatação ----

/// Barra visual com cor por limiar (verde <60%, amarelo <85%, vermelho).
pub fn bar(pct: f64, width: usize, color: bool) -> String {
    let pct = pct.clamp(0.0, 100.0);
    let filled = ((pct / 100.0) * width as f64).round() as usize;
    let bar_str = format!("{}{}", "█".repeat(filled), "░".repeat(width - filled));
    let col = if pct >= 85.0 {
        "red"
    } else if pct >= 60.0 {
        "yellow"
    } else {
        "green"
    };
    format!("[{}]", paint(&bar_str, col, color))
}

pub fn fmt_bytes(b: u64) -> String {
    const UNITS: [&str; 6] = ["B", "KiB", "MiB", "GiB", "TiB", "PiB"];
    if b == 0 {
        return "0 B".into();
    }
    let mut v = b as f64;
    let mut i = 0;
    while v >= 1024.0 && i < UNITS.len() - 1 {
        v /= 1024.0;
        i += 1;
    }
    let s = if v >= 100.0 {
        format!("{v:.0}")
    } else {
        format!("{v:.1}")
    };
    format!("{} {}", s.replace('.', ","), UNITS[i])
}

pub fn fmt_uptime(secs: f64) -> String {
    let s = secs as u64;
    let (h, m) = (s / 3600, (s % 3600) / 60);
    if h >= 24 {
        format!("{}d {}h", h / 24, h % 24)
    } else if h > 0 {
        format!("{h}h {m}min")
    } else {
        format!("{m}min")
    }
}

// ---- tabela box-drawing ----

/// Tabela com cantos `┌─┬┐` respeitando largura unicode aproximada.
pub fn table(headers: &[&str], rows: &[Vec<String>], color: bool) -> String {
    let ncols = headers.len();
    let mut widths: Vec<usize> = headers.iter().map(|h| h.chars().count()).collect();
    for r in rows {
        for (i, c) in r.iter().enumerate() {
            if i < ncols {
                widths[i] = widths[i].max(c.chars().count());
            }
        }
    }
    let rule = |l: &str, m: &str, r: &str| -> String {
        let mut s = String::from(l);
        for (i, w) in widths.iter().enumerate() {
            s.push_str(&"─".repeat(w + 2));
            s.push_str(if i + 1 == widths.len() { r } else { m });
        }
        s
    };
    let row_line = |cells: &[String]| -> String {
        let mut s = String::from("│");
        for (i, c) in cells.iter().enumerate() {
            let pad = widths.get(i).copied().unwrap_or(0).saturating_sub(c.chars().count());
            s.push(' ');
            s.push_str(c);
            s.push_str(&" ".repeat(pad + 1));
            s.push('│');
        }
        s
    };
    let mut out = String::new();
    out.push_str(&paint(&rule("┌", "┬", "┐"), "dim", color));
    out.push('\n');
    out.push_str(&paint(
        &row_line(&headers.iter().map(|h| paint(h, "bold", color)).collect::<Vec<_>>()),
        "dim",
        color,
    ));
    out.push('\n');
    out.push_str(&paint(&rule("├", "┼", "┤"), "dim", color));
    out.push('\n');
    for (n, r) in rows.iter().enumerate() {
        out.push_str(&row_line(r));
        out.push('\n');
        if n + 1 < rows.len() {
            out.push_str(&paint(&rule("├", "┼", "┤"), "dim", color));
            out.push('\n');
        }
    }
    out.push_str(&paint(&rule("└", "┴", "┘"), "dim", color));
    out
}
