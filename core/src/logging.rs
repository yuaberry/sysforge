//! Logging estruturado (tracing) com saída dupla: stderr + arquivo de log.
//! Arquivos ficam em ~/.local/share/sysforge/logs/.

use std::fs::{File, OpenOptions};
use std::io::Write as IoWrite;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};

use tracing_subscriber::fmt::MakeWriter;
use tracing_subscriber::EnvFilter;

pub struct LogWriter {
    file: Arc<Mutex<File>>,
    stderr_too: bool,
}

impl IoWrite for LogWriter {
    fn write(&mut self, buf: &[u8]) -> std::io::Result<usize> {
        if self.stderr_too {
            let _ = std::io::stderr().write_all(buf);
        }
        if let Ok(mut f) = self.file.lock() {
            let _ = f.write_all(buf);
            let _ = f.flush();
        }
        Ok(buf.len())
    }
    fn flush(&mut self) -> std::io::Result<()> {
        Ok(())
    }
}

pub struct LogMaker {
    file: Arc<Mutex<File>>,
    stderr_too: bool,
}

impl<'a> MakeWriter<'a> for LogMaker {
    type Writer = LogWriter;
    fn make_writer(&'a self) -> Self::Writer {
        LogWriter {
            file: Arc::clone(&self.file),
            stderr_too: self.stderr_too,
        }
    }
}

/// Inicializa o tracing. `log_file=None` → apenas stderr.
/// Respeita RUST_LOG; default é o filtro passado (ex.: "info").
/// Nunca entra em pânico se já foi inicializado (biblioteca compartilhada).
pub fn init(log_file: Option<&Path>, default_filter: &str) {
    let filter =
        EnvFilter::try_from_default_env().unwrap_or_else(|_| EnvFilter::new(default_filter));
    match log_file.and_then(|p| {
        if let Some(dir) = p.parent() {
            std::fs::create_dir_all(dir).ok();
        }
        OpenOptions::new().create(true).append(true).open(p).ok()
    }) {
        Some(f) => {
            let maker = LogMaker {
                file: Arc::new(Mutex::new(f)),
                stderr_too: true,
            };
            let _ = tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_ansi(false)
                .with_target(true)
                .with_writer(maker)
                .try_init();
        }
        None => {
            let _ = tracing_subscriber::fmt()
                .with_env_filter(filter)
                .with_ansi(true)
                .with_target(true)
                .try_init();
        }
    }
}

/// Caminho padrão do log do app: ~/.local/share/sysforge/logs/sysforge.log
pub fn default_app_log_file() -> Option<PathBuf> {
    let home = std::env::var_os("HOME")?;
    Some(
        PathBuf::from(home)
            .join(".local/share/sysforge/logs/sysforge.log"),
    )
}
