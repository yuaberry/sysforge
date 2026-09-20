//! Engine de boot UEFI — parser do efibootmgr.
//!
//! ESCRITA de entradas: Fase 1 NÃO escreve nada. Apenas leitura real.
//! Quando a escrita chegar (Milestone 1: BootNext one-shot), ela passa
//! pelo executor com Risk::LowRisk, snapshot de BootOrder ANTES da primeira
//! escrita e NUNCA altera BootOrder permanente — apenas BootNext.

use serde::{Deserialize, Serialize};

use crate::error::{ErrorDomain, YuaError};
use crate::executor::{CommandSpec, Executor};

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EfiBootEntry {
    /// ID de 4 dígitos hex, ex.: "0000".
    pub id: String,
    /// Entrada ativa (asterisco no efibootmgr).
    pub active: bool,
    /// Nome legível, ex.: "Ubuntu".
    pub name: String,
    /// Device path UEFI completo, ex.: "HD(1,GPT,...)/File(\EFI\...)".
    pub device_path: Option<String>,
    /// Caminho do loader \EFI\... dentro do device path (se houver File()).
    pub loader_path: Option<String>,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct EfiBootState {
    pub boot_current: Option<String>,
    pub timeout_secs: Option<u64>,
    pub boot_order: Vec<String>,
    pub entries: Vec<EfiBootEntry>,
}

/// Extrai o caminho de arquivo UEFI de `/File(\caminho)` — NÃO confundir com
/// `FvFile(uuid)` (referência a firmware volume, sem caminho de arquivo).
fn extract_loader(device_path: &str) -> Option<String> {
    let start = device_path.find("/File(")?;
    let rest = &device_path[start + 6..];
    let end = rest.find(')')?;
    let loader = &rest[..end];
    if loader.is_empty() {
        None
    } else {
        Some(loader.to_string())
    }
}

fn looks_like_entry_id(bytes: &[u8]) -> bool {
    bytes.len() == 4 && bytes.iter().all(|b| b.is_ascii_hexdigit())
}

/// Parser puro do texto do efibootmgr (formato util-linux):
///
/// ```text
/// BootCurrent: 0000
/// Timeout: 0 seconds
/// BootOrder: 0000,001C,0010
/// Boot0000* Ubuntu	HD(1,GPT,...)/File(\EFI\ubuntu\shimx64.efi)
/// Boot0010  Setup	FvFile(721c...)
/// ```
///
/// O device path vem NA MESMA LINHA após tab (efibootmgr >= 17) ou em
/// linha de continuação — suportamos ambos.
pub fn parse_efibootmgr(output: &str) -> EfiBootState {
    let mut state = EfiBootState::default();
    for line in output.lines() {
        let line = line.trim_end_matches('\r');
        if let Some(rest) = line.strip_prefix("BootCurrent:") {
            let v = rest.trim();
            if !v.is_empty() {
                state.boot_current = Some(v.to_string());
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("Timeout:") {
            if let Some(first) = rest.trim().split_whitespace().next() {
                state.timeout_secs = first.parse().ok();
            }
            continue;
        }
        if let Some(rest) = line.strip_prefix("BootOrder:") {
            state.boot_order = rest
                .trim()
                .split(',')
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
                .collect();
            continue;
        }
        // Entrada: "Boot" + 4 hex + [ '*' ] + ' ' + nome [ \t device-path ]
        if line.starts_with("Boot")
            && looks_like_entry_id(line.as_bytes().get(4..8).unwrap_or(&[]))
        {
            let id = line[4..8].to_string();
            let rest = &line[8..];
            let active = rest.starts_with('*');
            let rest = rest.trim_start_matches('*');
            let (name_part, devpath_part) = match rest.split_once('\t') {
                Some((n, d)) => (n.trim(), Some(d.trim().to_string())),
                None => (rest.trim(), None),
            };
            state.entries.push(EfiBootEntry {
                id,
                active,
                name: name_part.to_string(),
                device_path: devpath_part,
                loader_path: None,
            });
            continue;
        }
        // Continuação de device path (versões antigas: linha iniciada por tab).
        if line.starts_with('\t') || line.starts_with("    ") {
            if let Some(last) = state.entries.last_mut() {
                let chunk = line.trim();
                if !chunk.is_empty() {
                    let dp = last.device_path.get_or_insert_with(String::new);
                    if !dp.is_empty() {
                        dp.push(' ');
                    }
                    dp.push_str(chunk);
                }
            }
        }
    }
    for e in &mut state.entries {
        if let Some(dp) = &e.device_path {
            e.loader_path = extract_loader(dp);
        }
    }
    state
}

/// Leitura real do estado UEFI. Sem root funciona na maioria dos sistemas
/// (inclusive esta máquina); onde exigir, o erro sobe honesto (YUA-BOOT-001).
pub fn read_efi_state(executor: &Executor) -> Result<EfiBootState, YuaError> {
    let spec = CommandSpec::new("efibootmgr").timeout(std::time::Duration::from_secs(15));
    let r = executor.run_readonly(spec)?;
    if !r.success() {
        return Err(
            YuaError::new(ErrorDomain::Boot, 1, "Não foi possível ler as entradas de boot UEFI")
                .with_technical(format!(
                    "efibootmgr saiu com {:?}: {}",
                    r.exit_code,
                    r.stderr.trim()
                ))
                .with_recommendation(
                    "Em alguns sistemas a leitura do efivarfs exige root; nesse caso use o daemon em modo sistema (`yua daemon status`).",
                ),
        );
    }
    Ok(parse_efibootmgr(&r.stdout))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Saída REAL do efibootmgr desta máquina (recorte).
    const FIXTURE: &str = "BootCurrent: 0000\nTimeout: 0 seconds\nBootOrder: 0000,001C,0010,0011,0013,0016,0012,0014,0015,0018,0017,0019,001A,001B\nBoot0000* Ubuntu\tHD(1,GPT,5ce4e0d9-d391-4161-ab54-980fafc5ffea,0x800,0x100000)/File(\\EFI\\ubuntu\\shimx64.efi)\nBoot0010  Setup\tFvFile(721c8b66-426c-4e86-8e99-3457c46ab0b9)\nBoot0013* Hard Drive\tVenMsg(bc7838d2-0f82-4d60-8316-c068ee79d25b,f5b01cc8ce8e9841b3a8fb94b6dfefee)\nBoot001C* WIN_INSTALL\tPciRoot(0x0)/Pci(0x1f,0x2)/Sata(1,0,0)/HD(1,GPT,5ce4e0d9-d391-4161-ab54-980fafc5ffea,0x800,0x100000)/File()\n";

    #[test]
    fn parses_real_efibootmgr_output() {
        let s = parse_efibootmgr(FIXTURE);
        assert_eq!(s.boot_current.as_deref(), Some("0000"));
        assert_eq!(s.timeout_secs, Some(0));
        assert_eq!(&s.boot_order[..3], &["0000", "001C", "0010"]);
        assert_eq!(s.entries.len(), 4);

        let ubuntu = s.entries.iter().find(|e| e.id == "0000").unwrap();
        assert!(ubuntu.active);
        assert_eq!(ubuntu.name, "Ubuntu");
        assert_eq!(
            ubuntu.loader_path.as_deref(),
            Some("\\EFI\\ubuntu\\shimx64.efi")
        );
        assert!(ubuntu.device_path.as_deref().unwrap().starts_with("HD(1,GPT,5ce4e0d9"));

        let setup = s.entries.iter().find(|e| e.id == "0010").unwrap();
        assert!(!setup.active);
        assert_eq!(setup.name, "Setup");
        assert_eq!(setup.loader_path, None);
        assert!(setup.device_path.as_deref().unwrap().starts_with("FvFile("));

        let win = s.entries.iter().find(|e| e.id == "001C").unwrap();
        // Boot001C termina com File() vazio ⇒ loader None, device path presente.
        assert_eq!(win.loader_path, None);
        assert!(win.device_path.is_some());
    }

    #[test]
    fn live_read_works_unprivileged_here() {
        // Premissa validada nesta máquina: leitura sem root funciona.
        let ex = Executor::default();
        let s = read_efi_state(&ex).unwrap();
        assert_eq!(s.boot_current.as_deref(), Some("0000"));
        assert!(s.entries.iter().any(|e| e.name == "Ubuntu" && e.active));
        assert!(s.boot_order.contains(&"001C".to_string()));
    }
}
