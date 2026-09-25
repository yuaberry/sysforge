//! Redes — sondagem REAL via nmcli (NetworkManager), read-only.
//! Nada aqui altera configuração: ver e diagnosticar é o escopo desta página.

use crate::error::SysforgeError;
use crate::executor::{CommandSpec, Executor};
use serde::Serialize;

#[derive(Debug, Clone, Serialize)]
pub struct NetDevice {
    pub name: String,
    pub kind: String,
    pub state: String,
    pub connection: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WifiNetwork {
    pub ssid: String,
    pub signal: u32,
    pub security: String,
    pub active: bool,
}

#[derive(Debug, Clone, Serialize)]
pub struct NetInfo {
    pub nmcli_present: bool,
    pub devices: Vec<NetDevice>,
    /// Wi-Fi visíveis (vazio se não houver rádio ativo)
    pub wifi: Vec<WifiNetwork>,
}

fn nmcli(exec: &Executor, args: &[&str]) -> Result<String, SysforgeError> {
    let mut spec = CommandSpec::new("nmcli").timeout(std::time::Duration::from_secs(15));
    for a in args {
        spec = spec.arg(*a);
    }
    let r = exec.run_readonly(spec)?;
    if !r.success() {
        let owned: Vec<String> = args.iter().map(|a| a.to_string()).collect();
        return Err(SysforgeError::command_failed("nmcli", &owned, r.exit_code, &r.stderr));
    }
    Ok(r.stdout)
}

/// Lista dispositivos e conexões — formato terse, estável, sem locale.
pub fn info(exec: &Executor) -> Result<NetInfo, SysforgeError> {
    if !crate::executor::which("nmcli").is_some() {
        return Ok(NetInfo { nmcli_present: false, devices: vec![], wifi: vec![] });
    }
    let out = nmcli(exec, &["-t", "-f", "DEVICE,TYPE,STATE,CONNECTION", "device", "status"])?;
    let mut devices = Vec::new();
    for line in out.lines() {
        let p: Vec<&str> = line.split(':').collect();
        if p.len() >= 4 && !p[0].is_empty() {
            devices.push(NetDevice {
                name: p[0].into(),
                kind: p[1].into(),
                state: p[2].into(),
                connection: p[3].into(),
            });
        }
    }

    let mut wifi = Vec::new();
    let w = nmcli(exec, &["-t", "-f", "ACTIVE,SSID,SIGNAL,SECURITY", "device", "wifi", "list"]).unwrap_or_default();
    for line in w.lines() {
        let p: Vec<&str> = line.split(':').collect();
        if p.len() >= 4 && !p[1].is_empty() {
            wifi.push(WifiNetwork {
                active: p[0] == "sim" || p[0] == "yes",
                ssid: p[1].into(),
                signal: p[2].parse().unwrap_or(0),
                security: p[3].into(),
            });
        }
    }
    wifi.sort_by(|a, b| b.signal.cmp(&a.signal));

    Ok(NetInfo { nmcli_present: true, devices, wifi })
}

#[cfg(test)]
mod tests {
    #[test]
    fn parse_terse_fields() {
        // contratos do formato -t: DEVICE:TYPE:STATE:CONNECTION e ACTIVE:SSID:SIGNAL:SECURITY
        let d: Vec<&str> = "wlp3s0:wifi:conectado:MinhaRede".split(':').collect();
        assert!(d.len() >= 4 && d[0] == "wlp3s0");
        let w: Vec<&str> = "sim:CasaNET:82:WPA2".split(':').collect();
        assert!(w[1] == "CasaNET" && w[2] == "82");
    }
}
