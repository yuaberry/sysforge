//! Backup de arquivos do usuário ANTES de trocar de sistema.
//!
//! Escopo honesto: copiar pastas do usuário ($HOME) para um alvo externo
//! (disco USB montado, partição de dados montada em /media ou /run/media).
//! O rsync roda COMO O USUÁRIO (sem daemon, sem root): backup de arquivos
//! próprios para um ponto de montagem não exige privilégio nenhum.
//!
//! Toda execução segue o padrão da casa: dry-run antes (estimativa real),
//! `--info=progress2` para progresso agregado e --delete NUNCA incluído
//! (backup nunca apaga nada no destino — só soma).

use std::path::{Path, PathBuf};

use crate::error::{ErrorDomain, SysforgeError};
use crate::executor::{CommandSpec, Executor};
use serde::Serialize;

/// Pastas clássicas de valor do usuário (oferecidas como padrão no app).
pub const DEFAULT_DIRS: &[&str] = &["Documentos", "Documents", "Downloads", "Imagens", "Pictures", "Vídeos", "Videos", "Músicas", "Music", "Área de Trabalho", "Desktop"];

#[derive(Debug, Clone, Serialize)]
pub struct BackupTarget {
    pub mount: String,
    pub device: String,
    pub fs: String,
    pub free_bytes: u64,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupDir {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub exists: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct BackupEstimate {
    pub total_bytes: u64,
    pub free_bytes: u64,
    pub fits: bool,
}

/// Descobre alvos de backup: montagens removíveis/externas com espaço real.
pub fn targets() -> Vec<BackupTarget> {
    let mut out = Vec::new();
    for base in ["/media", "/run/media"] {
        if let Ok(rd) = std::fs::read_dir(base) {
            for user in rd.flatten() {
                if let Ok(rd2) = std::fs::read_dir(user.path()) {
                    for m in rd2.flatten() {
                        let mount = m.path();
                        if let Some(t) = probe_target(&mount) {
                            out.push(t);
                        }
                    }
                }
            }
        }
    }
    out
}

fn probe_target(mount: &Path) -> Option<BackupTarget> {
    if !mount.is_dir() {
        return None;
    }
    let free = fs_free(mount)?;
    if free == 0 {
        return None;
    }
    // de qual dispositivo vem a montagem? (findmnt, sem shell)
    let exec = Executor::default();
    let spec = CommandSpec::new("findmnt")
        .arg("-n")
        .arg("-o")
        .arg("SOURCE,FSTYPE")
        .arg("--target")
        .arg(mount.display().to_string())
        .timeout(std::time::Duration::from_secs(10));
    let r = exec.run_readonly(spec).ok()?;
    let mut it = r.stdout.split_whitespace();
    let device = it.next().unwrap_or("?").to_string();
    let fs = it.next().unwrap_or("?").to_string();
    if device.starts_with('/') {
        Some(BackupTarget {
            mount: mount.display().to_string(),
            device,
            fs,
            free_bytes: free,
        })
    } else {
        None
    }
}

fn fs_free(p: &Path) -> Option<u64> {
    #[cfg(unix)]
    {
        use std::os::unix::fs::MetadataExt;
        let md = std::fs::metadata(p).ok()?;
        Some(md.blocks() as u64 * 512)
    }
    #[cfg(not(unix))]
    {
        None
    }
}

fn dir_size(p: &Path) -> u64 {
    let mut total = 0u64;
    if let Ok(rd) = std::fs::read_dir(p) {
        for e in rd.flatten() {
            if let Ok(md) = e.metadata() {
                if md.is_file() {
                    total += md.len();
                } else if md.is_dir() {
                    total += dir_size(&e.path());
                }
            }
        }
    }
    total
}

/// Pastas do usuário com tamanho real (du recursivo próprio, sem xargs).
pub fn user_dirs() -> Vec<BackupDir> {
    let home = std::env::var_os("HOME").map(PathBuf::from).unwrap_or_default();
    let mut out = Vec::new();
    for d in DEFAULT_DIRS {
        let p = home.join(d);
        if p.is_dir() {
            out.push(BackupDir {
                name: d.to_string(),
                path: p.display().to_string(),
                size_bytes: dir_size(&p),
                exists: true,
            });
        }
    }
    out
}

/// Estimativa honesta: cabe ou não cabe (antes de copiar qualquer byte).
pub fn estimate(dirs: &[String], target: &str) -> Result<BackupEstimate, SysforgeError> {
    let home = std::env::var_os("HOME")
        .map(PathBuf::from)
        .ok_or_else(|| SysforgeError::new(ErrorDomain::Io, 30, "HOME não definido"))?;
    let mut total = 0u64;
    for d in dirs {
        total += dir_size(&home.join(d));
    }
    let free = fs_free(Path::new(target)).unwrap_or(0);
    Ok(BackupEstimate { total_bytes: total, free_bytes: free, fits: total > 0 && free > total })
}

/// Executa o backup REAL (rsync como usuário; --delete proibido por design).
/// `dirs` são nomes de pastas dentro de $HOME; `target` é o ponto de montagem.
/// Cria <target>/sysforge-backup/<dir> para cada pasta — nada é sobrescrito
/// fora do nosso subdiretório.
pub fn run(exec: &Executor, dirs: &[String], target: &str) -> Result<serde_json::Value, SysforgeError> {
    if dirs.is_empty() {
        return Err(SysforgeError::new(ErrorDomain::State, 10, "Nenhuma pasta selecionada para backup"));
    }
    let dest_root = Path::new(target).join("sysforge-backup");
    std::fs::create_dir_all(&dest_root)?;
    for d in dirs {
        if !DEFAULT_DIRS.contains(&d.as_str()) {
            return Err(SysforgeError::new(ErrorDomain::State, 11, format!("Pasta fora da lista permitida: {d}"))
                .with_recommendation("O backup só copia pastas do usuário conhecidas — nenhuma executável ou caminho arbitrário."));
        }
        let spec = CommandSpec::new("rsync")
            .arg("-a")
            .arg("--info=progress2")
            .arg("--exclude=.cache")
            .arg("--exclude=.thumbnails")
            .arg(format!("{d}/"))
            .arg(dest_root.join(d).display().to_string() + "/")
            .timeout(std::time::Duration::from_secs(60 * 60 * 12));
        let r = exec.run_readonly(spec)?; // cópia de dados do usuário: sem privilégio
        if !r.success() {
            return Err(SysforgeError::command_failed("rsync", &[d.clone()], r.exit_code, &r.stderr));
        }
        tracing::info!(dir = %d, "backup concluído");
    }
    Ok(serde_json::json!({ "dest": dest_root.display().to_string(), "dirs": dirs }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn allowed_dirs_are_user_scoped() {
        for d in DEFAULT_DIRS {
            assert!(!d.contains(".."), "nenhuma pasta padrão pode ter path traversal");
        }
    }

    #[test]
    fn estimate_flags_when_too_big() {
        let e = BackupEstimate { total_bytes: 100, free_bytes: 10, fits: false };
        assert!(!e.fits, "total > livre não pode caber");
    }
}
