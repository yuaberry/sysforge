//! Teste de integração REAL: sobe o daemon yua-osd em modo dev, conecta o
//! cliente IPC e valida o protocolo ponta a ponta — incluindo o FAIL-CLOSED
//! de métodos destrutivos (YUA-AUTH-002) e métodos desconhecidos.

use std::path::PathBuf;
use std::process::{Child, Command, Stdio};
use std::thread;
use std::time::Duration;

use serde_json::json;
use yua_core::error::YuaError;
use yua_core::ipc::protocol::{METHOD_DISKS_LIST, METHOD_ECHO, METHOD_EFI_ENTRIES, METHOD_SYSTEM_INFO};
use yua_core::ipc::YuaClient;

fn spawn_daemon(socket: &PathBuf) -> Child {
    let bin = env!("CARGO_BIN_EXE_yua-osd");
    Command::new(bin)
        .arg("--dev")
        .arg("--socket")
        .arg(socket)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .expect("falhou ao spawnar yua-osd")
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
fn dev_daemon_protocol_and_fail_closed() -> Result<(), YuaError> {
    let socket = std::env::temp_dir().join(format!("yua-osd-it-{}.sock", std::process::id()));
    let _ = std::fs::remove_file(&socket);
    let mut child = spawn_daemon(&socket);
    wait_socket(&socket);

    let result = (|| -> Result<(), YuaError> {
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

        // FAIL-CLOSED: método destrutivo recusado em modo dev — YUA-AUTH-002.
        let err = client
            .call("v1.disk.wipe", json!({"disk": "/dev/sda"}))
            .unwrap_err();
        assert_eq!(err.code, "YUA-AUTH-002");
        assert!(!err.recommendation.is_empty());

        // O mesmo vale para BootNext (escrita EFI) — registro destrutivo.
        let err = client
            .call("v1.boot.set_next", json!({"entry": "0000"}))
            .unwrap_err();
        assert_eq!(err.code, "YUA-AUTH-002");

        // Método desconhecido → YUA-NOTSUP-001, nunca "silêncio".
        let err = client.call("v1.metodo.inexistente", json!({})).unwrap_err();
        assert_eq!(err.code, "YUA-NOTSUP-001");

        Ok(())
    })();

    let _ = child.kill();
    let _ = child.wait();
    let _ = std::fs::remove_file(&socket);
    result
}
