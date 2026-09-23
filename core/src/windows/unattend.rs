//! Geração de `autounattend.xml` para Windows 11.
//!
//! POR QUE ISTO EXISTE: hardware antigo (como o i7-4510U, sem TPM) é barrado
//! nos requisitos oficiais do Win11. O método documentado pela comunidade —
//! e compatível com instalador oficial — são as chaves de registro
//! `HKLM\SYSTEM\Setup\LabConfig` (BypassTPMCheck/BypassSecureBootCheck/
//! BypassRAMCheck/BypassCPUCheck) aplicadas na fase windowsPE.
//!
//! SEGURANÇA: este gerador NUNCA escreve senhas no XML. Criação de conta
//! fica no OOBE (último passo manual, consciente).

use serde::{Deserialize, Serialize};

use crate::error::{ErrorDomain, SysforgeError};

/// Chaves genéricas de instalação (públicas, da documentação Microsoft —
/// não ativam nada, apenas selecionam a edição).
const KEY_HOME: &str = "YTMG3-N6DKC-DKB77-7M9GH-8HVX7";
const KEY_PRO: &str = "VK7JG-NPHTM-C97JM-9MPGT-3V66T";

pub const WINDOWS11_DOWNLOAD_URL: &str =
    "https://www.microsoft.com/pt-br/software-download/windows11";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct UnattendConfig {
    /// "home" | "pro"
    pub edition: String,
    /// Locale padrão pt-BR (formatos + teclado ABNT).
    pub locale: String,
    /// Fuso padrão de Brasília (sem DST desde 2019).
    pub timezone: String,
    /// MODO DESTRUtivo: apaga TODO o disco 0 (inclui Linux atual).
    /// Sem isto, o instalador pergunta onde instalar (usuário decide).
    pub full_disk_wipe: bool,
    /// Bypass de TPM/SecureBoot/RAM/CPU (LabConfig) — para hardware antigo.
    pub bypass_requirements: bool,
}

impl Default for UnattendConfig {
    fn default() -> Self {
        Self {
            edition: "pro".into(),
            locale: "pt-BR".into(),
            timezone: "E. South America Standard Time".into(),
            full_disk_wipe: false,
            bypass_requirements: true,
        }
    }
}

fn esc(s: &str) -> String {
    s.replace('&', "&").replace('<', "<").replace('>', ">")
}

fn product_key(edition: &str) -> &'static str {
    if edition.eq_ignore_ascii_case("home") {
        KEY_HOME
    } else {
        KEY_PRO
    }
}

/// Gera o autounattend.xml completo como String.
pub fn generate_autounattend(cfg: &UnattendConfig) -> Result<String, SysforgeError> {
    if !matches!(cfg.edition.to_lowercase().as_str(), "home" | "pro") {
        return Err(SysforgeError::new(
            ErrorDomain::NotSupported,
            2,
            format!("Edição desconhecida: {}", cfg.edition),
        )
        .with_recommendation("Use \"home\" ou \"pro\"."));
    }

    let locale = esc(&cfg.locale);
    let timezone = esc(&cfg.timezone);
    let key = product_key(&cfg.edition);
    let edition_name = if cfg.edition.eq_ignore_ascii_case("home") {
        "Windows 11 Home"
    } else {
        "Windows 11 Pro"
    };

    let mut bypass_cmds = String::new();
    if cfg.bypass_requirements {
        for (i, v) in [
            "BypassTPMCheck",
            "BypassSecureBootCheck",
            "BypassRAMCheck",
            "BypassCPUCheck",
        ]
        .iter()
        .enumerate()
        {
            bypass_cmds.push_str(&format!(
                r#"        <RunSynchronousCommand wcm:action="add">
          <Order>{}</Order>
          <Path>reg add HKLM\SYSTEM\Setup\LabConfig /v {} /t REG_DWORD /d 1 /f</Path>
          <Description>SYSFORGE: bypass de requisito oficial do Windows 11</Description>
        </RunSynchronousCommand>
"#,
                i + 1,
                v
            ));
        }
    }

    // Configuração de disco só existe em modo full-wipe — caso contrário a
    // escolha de partição é MANUAL no instalador (segurança em primeiro lugar).
    let disk_config = if cfg.full_disk_wipe {
        format!(            r#"      <DiskConfiguration>
        <Disk wcm:action="add">
          <DiskID>0</DiskID>
          <WillWipeDisk>true</WillWipeDisk>
          <CreatePartitions>
            <CreatePartition wcm:action="add">
              <Order>1</Order><Type>EFI</Type><Size>300</Size>
            </CreatePartition>
            <CreatePartition wcm:action="add">
              <Order>2</Order><Type>MSR</Type><Size>16</Size>
            </CreatePartition>
            <CreatePartition wcm:action="add">
              <Order>3</Order><Type>Primary</Type><Extend>true</Extend>
            </CreatePartition>
          </CreatePartitions>
          <ModifyPartitions>
            <ModifyPartition wcm:action="add">
              <Order>1</Order><PartitionID>1</PartitionID><Format>FAT32</Format><Label>System</Label>
            </ModifyPartition>
            <ModifyPartition wcm:action="add">
              <Order>2</Order><PartitionID>3</PartitionID><Format>NTFS</Format><Label>Windows</Label>
            </ModifyPartition>
          </ModifyPartitions>
        </Disk>
      </DiskConfiguration>
      <ImageInstall>
        <OSImage>
          <InstallFrom>
            <MetaData wcm:action="add">
              <Key>/IMAGE/NAME</Key>
              <Value>{edition_name}</Value>
            </MetaData>
          </InstallFrom>
          <InstallTo>
            <DiskID>0</DiskID><PartitionID>3</PartitionID>
          </InstallTo>
        </OSImage>
      </ImageInstall>
"#)
    } else {
        String::new()
    };

    let wipe_note = if cfg.full_disk_wipe {
        " (MODO APAGAR DISCO 0 ATIVO)"
    } else {
        ""
    };

    Ok(format!(
        r#"<?xml version="1.0" encoding="utf-8"?>
<!-- autounattend.xml gerado pelo SYSFORGE -->
<!-- ATENÇÃO{wipe_note}: este arquivo automatiza a instalação do Windows 11. -->
<unattend xmlns="urn:schemas-microsoft-com:unattend">
  <settings pass="windowsPE">
    <component name="Microsoft-Windows-Setup" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
{bypass_cmds}      <UserData>
        <AcceptEula>true</AcceptEula>
        <ProductKey>
          <Key>{key}</Key>
          <WillShowUI>OnError</WillShowUI>
        </ProductKey>
      </UserData>
{disk_config}    </component>
    <component name="Microsoft-Windows-International-Core-WinPE" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
      <SetupUILanguage>
        <UILocale>{locale}</UILocale>
      </SetupUILanguage>
      <InputLocale>0416:00000416</InputLocale>
      <SystemLocale>{locale}</SystemLocale>
      <UILanguage>{locale}</UILanguage>
      <UserLocale>{locale}</UserLocale>
    </component>
  </settings>
  <settings pass="specialize">
    <component name="Microsoft-Windows-Shell-Setup" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
      <TimeZone>{timezone}</TimeZone>
    </component>
    <component name="Microsoft-Windows-Deployment" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
      <RunSynchronous>
        <RunSynchronousCommand wcm:action="add">
          <Order>1</Order>
          <Path>reg add HKLM\SOFTWARE\Microsoft\Windows\CurrentVersion\OOBE /v BypassNRO /t REG_DWORD /d 1 /f</Path>
          <Description>SYSFORGE: permite conta local no OOBE (se o instalador pedir conta Microsoft, desconecte a rede)</Description>
        </RunSynchronousCommand>
      </RunSynchronous>
    </component>
  </settings>
  <settings pass="oobeSystem">
    <component name="Microsoft-Windows-International-Core" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
      <InputLocale>0416:00000416</InputLocale>
      <SystemLocale>{locale}</SystemLocale>
      <UILanguage>{locale}</UILanguage>
      <UserLocale>{locale}</UserLocale>
    </component>
    <component name="Microsoft-Windows-Shell-Setup" processorArchitecture="amd64" publicKeyToken="31bf3856ad364e35" language="neutral" versionScope="nonSxS">
      <OOBE>
        <HideEULAPage>true</HideEULAPage>
        <HideOemRegistrationScreen>true</HideOemRegistrationScreen>
        <HideOnlineAccountScreens>true</HideOnlineAccountScreens>
        <HideWirelessSetupInOOBE>false</HideWirelessSetupInOOBE>
        <ProtectYourPC>3</ProtectYourPC>
      </OOBE>
    </component>
  </settings>
</unattend>
"#
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn generates_bypass_and_ptbr_defaults() {
        let xml = generate_autounattend(&UnattendConfig::default()).unwrap();
        assert!(xml.contains(r#"<?xml version="1.0""#));
        assert!(xml.contains("BypassTPMCheck"));
        assert!(xml.contains("BypassSecureBootCheck"));
        assert!(xml.contains("BypassRAMCheck"));
        assert!(xml.contains("BypassCPUCheck"));
        assert!(xml.contains(KEY_PRO), "edição default pro");
        assert!(xml.contains("pt-BR"));
        assert!(xml.contains("E. South America Standard Time"));
        assert!(!xml.contains("WillWipeDisk"), "sem full_disk_wipe NÃO apaga disco");
        assert!(!xml.contains("<Password"), "nunca embutimos senhas");
    }

    #[test]
    fn home_edition_uses_home_key() {
        let mut cfg = UnattendConfig::default();
        cfg.edition = "home".into();
        let xml = generate_autounattend(&cfg).unwrap();
        assert!(xml.contains(KEY_HOME));
        assert!(!xml.contains(KEY_PRO));
        // O nome da edição entra no ImageInstall (modo full-wipe):
        let mut wipe_cfg = cfg.clone();
        wipe_cfg.full_disk_wipe = true;
        let xml_wipe = generate_autounattend(&wipe_cfg).unwrap();
        assert!(xml_wipe.contains("Windows 11 Home"));
        assert!(!xml_wipe.contains("Windows 11 Pro"));
    }

    #[test]
    fn full_wipe_adds_disk_configuration_with_loud_warning() {
        let mut cfg = UnattendConfig::default();
        cfg.full_disk_wipe = true;
        let xml = generate_autounattend(&cfg).unwrap();
        assert!(xml.contains("WillWipeDisk"));
        assert!(xml.contains("MODO APAGAR DISCO 0 ATIVO"));
        assert!(xml.contains("<PartitionID>3</PartitionID>"));
    }

    #[test]
    fn rejects_unknown_edition() {
        let mut cfg = UnattendConfig::default();
        cfg.edition = "enterprise-n".into();
        assert_eq!(generate_autounattend(&cfg).unwrap_err().code, "SF-NOTSUP-002");
    }
}
