//! Teste de integração REAL: sobe o daemon sysforge-osd em modo dev, conecta o
//! cliente IPC e valida o protocolo ponta a ponta — incluindo o FAIL-CLOSED
//! de métodos destrutivos (SF-AUTH-002) e métodos desconhecidos.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use serde_json::json;
use sysforge_core::error::SysforgeError;
use sysforge_core::ipc::protocol::{
    METHOD_BOOT_SET_NEXT, METHOD_BOOT_SNAPSHOT, METHOD_DISKS_LIST, METHOD_ECHO,
    METHOD_EFI_ENTRIES, METHOD_SYSTEM_INFO,
};
use sysforge_core::ipc::YuaClient;

fn spawn_daemon(socket: &PathBuf) -> Child {
    let bin = env!("CARGO_BIN_EXE_sysforge-osd");
    Command::new(bin)
        .arg("--dev")
        .arg("--socket")
        .arg(socket)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("falhou ao spawnar sysforge-osd")
}

fn wait_socket(socket: &PathBuf) {
    for _ in 0..100 {
        if socket.exists() {
            return;
        }
        thread::sleep(Duration::from_millis(50));
    }
    panic!("daemon não criou o socket a tempo");
}

#[test]
fn dev_daemon_protocol_and_fail_closed() -> Result<(), SysforgeError> {
    let socket = std::env::temp_dir().join(format!("sysforge-osd-it-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket);
    let mut child = spawn_daemon(&socket);
    wait_socket(&socket);

    let result = (|| -> Result<(), SysforgeError> {
        let mut client = YuaClient::connect(&socket)?;

        // v1.echo — ida e volta
        let pong = client.call(METHOD_ECHO, json!({"message": "teste-integracao"}))?;
        assert_eq!(pong["echo"], "teste-integracao");

        // v1.system.info — dados REAIS da máquina
        let sys = client.call(METHOD_SYSTEM_INFO, json!({}))?;
        assert!(sys["hostname"].as_str().is_some());
        assert!(sys["os"]["pretty_name"].as_str().is_some());
        assert!(sys["is_uefi"].as_bool() == Some(true), "esta máquina boota em UEFI nativo");

        // v1.disks.list — o disco físico real do dev box
        let disks = client.call(METHOD_DISKS_LIST, json!({}))?;
        let arr = disks.as_array().expect("disks.list retorna array");
        assert!(
            arr.iter().any(|d| d["name"] == "sda" && d["serial"] == "WX61A79A2TDH"),
            "serial do WDC real deve vir enriquecido do udev"
        );

        // v1.efi.entries — entrada Ubuntu real presente
        let efi = client.call(METHOD_EFI_ENTRIES, json!({}))?;
        assert!(efi["boot_current"] == "0000");
        assert!(
            efi["entries"]
                .as_array()
                .unwrap()
                .iter()
                .any(|e| e["name"] == "Ubuntu")
        );

        // FAIL-CLOSED: método destrutivo recusado em modo dev — SF-AUTH-002.
        let err = client
            .call("v1.disk.wipe", json!({"disk": "/dev/sda"}))
            .unwrap_err();
        assert_eq!(err.code, "SF-AUTH-002");
        assert!(!err.recommendation.is_empty());

        // BootNext agora EXISTE, mas em dev continua fail-closed (SF-AUTH-002).
        let err = client
            .call(METHOD_BOOT_SET_NEXT, json!({"entry_id": "0000", "confirm": true}))
            .unwrap_err();
        assert_eq!(err.code, "SF-AUTH-002");

        // Snapshot é read-only ⇒ permitido em dev, com arquivo real gravado.
        let snap = client.call(METHOD_BOOT_SNAPSHOT, json!({}))?;
        assert!(snap["snapshot_path"].as_str().is_some());
        assert!(snap["state"]["entries"].as_array().unwrap().len() >= 13);

        // Método desconhecido → SF-NOTSUP-001, nunca "silêncio".
        let err = client.call("v1.metodo.inexistente", json!({})).unwrap_err();
        assert_eq!(err.code, "SF-NOTSUP-001");

        Ok(())
    })();

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&socket);
    result
}
