//! Controle de energia e "reiniciar direto na BIOS".
//!
//! COMO "ENTRAR NA BIOS" FUNCIONA DE VERDADE: a UEFI define a variável
//! `OsIndications` (namespace EFI_GLOBAL_VARIABLE). Quando o SO escreve nela
//! o bit `EFI_OS_INDICATIONS_BOOT_TO_FIRMWARE_UI` e a variável é
//! NÃO-VOLÁTIL, no próximo boot o firmware abre a tela de SETUP em vez do
//! bootloader — sem precisar apertar F2. É exatamente o mecanismo usado por
//! `systemctl reboot --firmware-setup` e por instaladores como o da MSI.
//!
//! Pré-requisitos: leitura de `OsIndicationsSupported` confirma se o firmware
//! honra o bit. Escrita exige ROOT (via daemon em modo sistema).

use std::fs;
use std::io::Write;
use std::path::Path;

use crate::error::{ErrorDomain, YuaError};
use crate::executor::{CommandSpec, Executor, ExecResult};

/// OsIndicationsSupported-8be4df61-... (o firmware anuncia o que aceita).
pub const EFIVAR_OS_INDICATIONS_SUPPORTED: &str =
    "/sys/firmware/efi/efivars/OsIndicationsSupported-8be4df61-93ca-11d2-aa0d-00e098032b8c";
/// OsIndications-8be4df61-... (o SO pede ações para o próximo boot).
pub const EFIVAR_OS_INDICATIONS: &str =
    "/sys/firmware/efi/efivars/OsIndications-8be4df61-93ca-11d2-aa0d-00e098032b8c";

/// Bit 0: abrir a UI de setup do firmware no próximo boot.
pub const INDICATION_BOOT_TO_FIRMWARE_UI: u64 = 1;

/// Lê um efivar como u64 LE (formato efivarfs: 4 bytes de atributos + valor).
fn read_var_u64(path: &str) -> Result<Option<u64>, YuaError> {
    match fs::read(path) {
        Ok(bytes) if bytes.len() >= 12 => {
            let mut buf = [0u8; 8];
            buf.copy_from_slice(&bytes[4..12]);
            Ok(Some(u64::from_le_bytes(buf)))
        }
        Ok(_) => Ok(None),
        // Sem permissão de leitura ou variável inexistente → None honesto.
        Err(_) => Ok(None),
    }
}

/// O firmware suporta "reiniciar direto no setup"?
pub fn firmware_reboot_supported() -> bool {
    matches!(read_var_u64(EFIVAR_OS_INDICATIONS_SUPPORTED), Ok(Some(mask)) if mask & INDICATION_BOOT_TO_FIRMWARE_UI != 0)
}

/// Escreve o bit BOOT_TO_FIRMWARE_UI em OsIndications (exige root).
/// A variável é NÃO-VOLÁTIL: persiste até o firmware consumir no boot.
pub fn arm_firmware_reboot() -> Result<(), YuaError> {
    let payload = {
        // Atributos: NON_VOLATILE(0x01) | BOOTSERVICE_ACCESS(0x02) | RUNTIME_ACCESS(0x04) = 7.
        // Não-volátil é ESSENCIAL: o firmware só vê o pedido no PRÓXIMO boot.
        let attrs: u32 = 0x07;
        let value: u64 = INDICATION_BOOT_TO_FIRMWARE_UI;
        let mut buf = Vec::with_capacity(12);
        buf.extend_from_slice(&attrs.to_le_bytes());
        buf.extend_from_slice(&value.to_le_bytes());
        buf
    };
    let path = Path::new(EFIVAR_OS_INDICATIONS);
    let mut f = fs::OpenOptions::new()
        .write(true)
        .create(true)
        .truncate(true)
        .open(path)
        .map_err(|e| {
            YuaError::new(
                ErrorDomain::Uefi,
                2,
                "Não foi possível armar o reboot para o firmware (BIOS)",
            )
            .with_technical(format!("escrever {}: {e}", EFIVAR_OS_INDICATIONS))
            .with_recommendation("A escrita exige root. Use o daemon em modo sistema (`yua daemon system`) — o polkit pedirá sua senha.")
        })?;
    f.write_all(&payload)?;
    f.flush()?;
    Ok(())
}

/// Reinicia a máquina AGORA (systemctl reboot). Exige daemon em modo sistema.
pub fn system_reboot(executor: &Executor) -> Result<ExecResult, YuaError> {
    let spec = CommandSpec::new("systemctl")
        .arg("reboot")
        .timeout(std::time::Duration::from_secs(30));
    executor.run_low_risk(spec)
}

/// Desliga a máquina AGORA (systemctl poweroff).
pub fn system_poweroff(executor: &Executor) -> Result<ExecResult, YuaError> {
    let spec = CommandSpec::new("systemctl")
        .arg("poweroff")
        .timeout(std::time::Duration::from_secs(30));
    executor.run_low_risk(spec)
}

/// Reinicia DIRETO na tela de setup do firmware (BIOS/UEFI).
/// Caminho primário: `systemctl reboot --firmware-setup` (implementação
/// systemd, testada em milhões de máquinas). Fallback: escrever OsIndications
/// na mão + reboot (firmwares sem systemd caminho — não é o caso no Mint).
pub fn reboot_to_firmware(executor: &Executor) -> Result<(), YuaError> {
    if !firmware_reboot_supported() {
        return Err(YuaError::new(
            ErrorDomain::Uefi,
            1,
            "Este firmware não anuncia suporte a reboot-para-setup",
        )
        .with_technical("bit BOOT_TO_FIRMWARE_UI ausente em OsIndicationsSupported")
        .with_recommendation("Entre no setup manualmente no boot (F2/Del) ou atualize o firmware."));
    }
    let spec = CommandSpec::new("systemctl")
        .arg("reboot")
        .arg("--firmware-setup")
        .timeout(std::time::Duration::from_secs(30));
    let r = executor.run_low_risk(spec)?;
    if !r.success() {
        // Fallback: armamos manualmente e reiniciamos.
        tracing::warn!("systemctl --firmware-setup falhou; tentando OsIndications manual");
        arm_firmware_reboot()?;
        system_reboot(executor)?;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn firmware_reboot_support_detected_on_this_machine() {
        // Premissa validada nesta máquina: Dell Vostro 5470 (UEFI) anuncia
        // BOOT_TO_FIRMWARE_UI em OsIndicationsSupported. Se algum dia falhar,
        // é mudança real do firmware que o doctor deve reportar.
        assert!(
            firmware_reboot_supported(),
            "esperado: firmware Dell suporta reboot-para-setup"
        );
    }

    #[test]
    fn arm_requires_root_and_fails_honestly() {
        // Sem root (nossa sessão), a escrita DEVE recusar com YUA-UEFI-002 —
        // garantindo que nada "arma BIOS" silenciosamente sem privilégio.
        let err = arm_firmware_reboot().unwrap_err();
        assert_eq!(err.code, "YUA-UEFI-002");
        assert!(err.recommendation.contains("daemon"));
    }
}
