//! Detecção de mídia removível (USB/SD) e utilitários de cópia com progresso.
//! Detecção via lsblk (rm=true) — read-only, sem root.

use std::io::{Read, Write};
use std::path::Path;

use serde::{Deserialize, Serialize};

use crate::disk::lsblk::list_blockdevices;
use crate::error::SysforgeError;
use crate::executor::Executor;

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RemovableMedia {
    pub name: String,
    pub path: String,
    pub size_bytes: u64,
    pub fstype: Option<String>,
    /// Ponto de montagem da primeira partição montada (se houver).
    pub mounted_at: Option<String>,
    /// true quando há diretório `ventoy/` na raiz montada.
    pub is_ventoy: bool,
    pub model: Option<String>,
}

/// Lista pendrives/SDs reais conectados (rm=true no lsblk).
pub fn list_removable_media(executor: &Executor) -> Result<Vec<RemovableMedia>, SysforgeError> {
    let devs = list_blockdevices(executor)?;
    let mut out = Vec::new();
    for d in devs.iter().filter(|d| d.is_disk() && d.rm.unwrap_or(false)) {
        // Primeira partição montada (Ventoy costuma ter a 1ª partição montável).
        let (fstype, mounted_at) = d
            .children
            .iter()
            .find_map(|p| {
                p.mounts()
                    .first()
                    .map(|m| (p.fstype.clone(), Some(m.clone())))
            })
            .unwrap_or((d.children.first().and_then(|p| p.fstype.clone()), None));

        // Ventoy: diretório `ventoy` na raiz da partição montada.
        let is_ventoy = mounted_at
            .as_ref()
            .map(|m| Path::new(m).join("ventoy").is_dir())
            .unwrap_or(false);

        out.push(RemovableMedia {
            name: d.name.clone(),
            path: d.path.clone().unwrap_or_else(|| format!("/dev/{}", d.name)),
            size_bytes: d.size,
            fstype,
            mounted_at,
            is_ventoy,
            model: d.model_trimmed(),
        });
    }
    Ok(out)
}

/// Cópia de arquivo com retorno de progresso (bytes copiados).
/// Progresso REPORTADO DE VERDADE — nenhum byte inventado.
pub fn copy_with_progress<F: FnMut(u64, u64)>(
    src: &Path,
    dst: &Path,
    mut on_progress: F,
) -> Result<u64, SysforgeError> {
    let total = std::fs::metadata(src).map(|m| m.len()).unwrap_or(0);
    let mut f = std::fs::File::open(src)?;
    let mut out = std::fs::File::create(dst)?;
    let mut buf = vec![0u8; 4 * 1024 * 1024];
    let mut done: u64 = 0;
    loop {
        let n = f.read(&mut buf)?;
        if n == 0 {
            break;
        }
        out.write_all(&buf[..n])?;
        done += n as u64;
        on_progress(done, total);
    }
    out.flush()?;
    Ok(done)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write as _;

    #[test]
    fn no_removable_media_on_dev_machine_is_honest() {
        // Máquina do dev: apenas /dev/sda (não removível) ⇒ lista vazia.
        let ex = Executor::default();
        let media = list_removable_media(&ex).unwrap();
        assert!(
            media.iter().all(|m| m.name != "sda"),
            "sda é fixo — nunca deve aparecer como removível"
        );
    }

    #[test]
    fn copy_with_progress_reports_real_bytes() {
        let dir = std::env::temp_dir();
        let src = dir.join(format!("sysforge-src-{}.bin", std::process::id()));
        let dst = dir.join(format!("sysforge-dst-{}.bin", std::process::id()));
        let mut f = std::fs::File::create(&src).unwrap();
        f.write_all(&[7u8; 1024 * 256]).unwrap();
        drop(f);
        let mut last = (0u64, 0u64);
        let n = copy_with_progress(&src, &dst, |d, t| last = (d, t)).unwrap();
        assert_eq!(n, 256 * 1024);
        assert_eq!(last.0, n);
        assert_eq!(last.1, n);
        std::fs::remove_file(&src).ok();
        std::fs::remove_file(&dst).ok();
    }
}
