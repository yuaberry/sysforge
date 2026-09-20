//! Engine de inventário de blocos via `lsblk --json -b` (util-linux 2.39+).
//!
//! POR QUE lsblk e não /sys direto: uma passada retorna a árvore completa
//! (disco → partições) com modelo, serial, FS e montagens — consistente
//! entre kernel/naming (nvme0n1p2 etc.). Com `-b` os tamanhos vêm em bytes
//! exatos; com `LC_ALL=C` o parsing é estável (executor já garante).

use serde::{Deserialize, Serialize};

use crate::error::{ErrorDomain, YuaError};
use crate::executor::{CommandSpec, Executor};

/// Colunas fixas — nomes estáveis entre versões suportadas.
pub const LSBLK_FIELDS: &str = "NAME,PATH,MAJ:MIN,TYPE,FSTYPE,MOUNTPOINTS,SIZE,MODEL,SERIAL,UUID,PARTLABEL,PARTUUID,TRAN,RO,RM";

/// Dispositivo de bloco (disco ou partição), espelhando o JSON do lsblk.
/// Campos opcionais usam `#[serde(default)]` para tolerar variações.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct LsblkDevice {
    pub name: String,
    #[serde(default)]
    pub path: Option<String>,
    #[serde(rename = "maj:min", default)]
    pub majmin: Option<String>,
    #[serde(rename = "type", default)]
    pub kind: Option<String>,
    #[serde(default)]
    pub fstype: Option<String>,
    /// util-linux >= 2.39: array (entradas `null` quando não montado).
    #[serde(default)]
    pub mountpoints: Option<Vec<Option<String>>>,
    /// Fallback para util-linux antigo.
    #[serde(default)]
    pub mountpoint: Option<String>,
    #[serde(default)]
    pub size: u64,
    #[serde(default)]
    pub model: Option<String>,
    #[serde(default)]
    pub serial: Option<String>,
    #[serde(default)]
    pub uuid: Option<String>,
    #[serde(default)]
    pub partlabel: Option<String>,
    #[serde(default)]
    pub partuuid: Option<String>,
    #[serde(default)]
    pub tran: Option<String>,
    #[serde(default)]
    pub ro: Option<bool>,
    #[serde(default)]
    pub rm: Option<bool>,
    #[serde(default)]
    pub children: Vec<LsblkDevice>,
}

impl LsblkDevice {
    pub fn is_disk(&self) -> bool {
        self.kind.as_deref() == Some("disk")
    }

    pub fn mounts(&self) -> Vec<String> {
        if let Some(arr) = &self.mountpoints {
            return arr.iter().flatten().cloned().collect();
        }
        self.mountpoint.clone().into_iter().collect()
    }

    pub fn model_trimmed(&self) -> Option<String> {
        self.model.as_ref().map(|m| m.trim().to_string()).filter(|m| !m.is_empty())
    }

    pub fn serial_trimmed(&self) -> Option<String> {
        self.serial.as_ref().map(|s| s.trim().to_string()).filter(|s| !s.is_empty())
    }
}

#[derive(Debug, Deserialize)]
struct LsblkRoot {
    #[serde(default)]
    blockdevices: Vec<LsblkDevice>,
}

/// Parser puro do JSON do lsblk (testável sem executar nada).
pub fn parse_lsblk_json(json: &str) -> Result<Vec<LsblkDevice>, YuaError> {
    let root: LsblkRoot = serde_json::from_str(json)?;
    Ok(root.blockdevices)
}

fn lsblk_spec(executor_extra: Option<String>) -> CommandSpec {
    let mut spec = CommandSpec::new("lsblk")
        .arg("--json")
        .arg("-b")
        .arg("-o")
        .arg(LSBLK_FIELDS)
        .timeout(std::time::Duration::from_secs(20));
    if let Some(dev) = executor_extra {
        spec = spec.arg(dev);
    }
    spec
}

/// Árvore completa de dispositivos de bloco.
pub fn list_blockdevices(executor: &Executor) -> Result<Vec<LsblkDevice>, YuaError> {
    let r = executor.run_readonly(lsblk_spec(None))?;
    if !r.success() {
        return Err(YuaError::command_failed("lsblk", &[], r.exit_code, &r.stderr));
    }
    parse_lsblk_json(&r.stdout)
}

 /// Dispositivo único (disco OU partição) por caminho /dev/....
pub fn probe_device(executor: &Executor, path: &str) -> Result<LsblkDevice, YuaError> {
    let r = executor.run_readonly(lsblk_spec(Some(path.to_string())))?;
    if !r.success() {
        return Err(
            YuaError::new(ErrorDomain::Disk, 1, format!("Dispositivo {path} não encontrado"))
                .with_technical(format!("lsblk saiu com {:?}: {}", r.exit_code, r.stderr.trim()))
                .with_recommendation("Verifique se o dispositivo está conectado e o caminho está correto (ex.: /dev/sdb)."),
        );
    }
    let devs = parse_lsblk_json(&r.stdout)?;
    devs.into_iter()
        .next()
        .ok_or_else(|| YuaError::new(ErrorDomain::Disk, 1, format!("lsblk não retornou dados para {path}")))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Recorte REAL da saída `lsblk --json -b` desta máquina (WDC WD10SPZX).
    const FIXTURE: &str = r#"{
      "blockdevices": [
         {
            "name": "sda", "path": "/dev/sda", "maj:min": "8:0", "type": "disk",
            "fstype": null, "mountpoints": [null], "size": 1000204886016,
            "model": "WDC WD10SPZX-75Z10T2", "serial": "WX61A79A2TDH",
            "uuid": null, "partlabel": null, "partuuid": null,
            "tran": "sata", "ro": false, "rm": false,
            "children": [
               {
                  "name": "sda1", "path": "/dev/sda1", "maj:min": "8:1", "type": "part",
                  "fstype": "vfat", "mountpoints": ["/boot/efi"], "size": 536870912,
                  "model": null, "serial": null, "uuid": "C065-50C0",
                  "partlabel": "EFI System Partition",
                  "partuuid": "5ce4e0d9-d391-4161-ab54-980fafc5ffea",
                  "tran": null, "ro": false, "rm": false
               },{
                  "name": "sda2", "path": "/dev/sda2", "maj:min": "8:2", "type": "part",
                  "fstype": "ext4", "mountpoints": ["/"], "size": 999666221056,
                  "model": null, "serial": null, "uuid": "1e1513bd-7e41-4ed1-a307-7797dd0720dc",
                  "partlabel": null,
                  "partuuid": "3e3d6a23-95c2-4f38-b82e-f39f21aa0974",
                  "tran": null, "ro": false, "rm": false
               }
            ]
         }
      ]
    }"#;

    #[test]
    fn parses_real_fixture() {
        let devs = parse_lsblk_json(FIXTURE).unwrap();
        assert_eq!(devs.len(), 1);
        let sda = &devs[0];
        assert!(sda.is_disk());
        assert_eq!(sda.size, 1000204886016);
        assert_eq!(sda.model_trimmed().as_deref(), Some("WDC WD10SPZX-75Z10T2"));
        assert_eq!(sda.serial_trimmed().as_deref(), Some("WX61A79A2TDH"));
        assert_eq!(sda.mounts().len(), 0);
        assert_eq!(sda.children.len(), 2);
        assert_eq!(sda.children[0].mounts(), vec!["/boot/efi"]);
        assert_eq!(sda.children[1].mounts(), vec!["/"]);
        assert_eq!(sda.children[0].partlabel.as_deref(), Some("EFI System Partition"));
        assert_eq!(sda.children[1].fstype.as_deref(), Some("ext4"));
        assert_eq!(sda.majmin.as_deref(), Some("8:0"));
    }

    #[test]
    fn live_machine_has_esp_and_root() {
        // Teste contra a máquina REAL (read-only): o alvo de desenvolvimento
        // tem exatamente 1 disco com ESP em sda1 e rootfs em sda2.
        let ex = crate::executor::Executor::default();
        let devs = list_blockdevices(&ex).unwrap();
        let disks: Vec<_> = devs.iter().filter(|d| d.is_disk()).collect();
        assert_eq!(disks.len(), 1, "máquina de dev tem disco único /dev/sda");
        let parts = &disks[0].children;
        assert!(parts.iter().any(|p| p.mounts().iter().any(|m| m == "/boot/efi")));
        assert!(parts.iter().any(|p| p.mounts().iter().any(|m| m == "/")));
    }
}
