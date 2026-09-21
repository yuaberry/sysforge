//! Garantia do daemon em modo SISTEMA — compartilhado por CLI e app desktop.
//!
//! Fluxo: se o socket já responde, usa. Senão, sobe via `pkexec yua-osd
//! --system` — o polkit abre o diálogo de senha NA TELA do usuário (agente
//! da sessão). O daemon nunca vê a senha; o usuário é quem autoriza.

use std::path::PathBuf;
use std::process::Stdio;
use std::time::{Duration, Instant};

use crate::error::{ErrorDomain, YuaError};
use crate::executor::which;
use crate::ipc::client::YuaClient;
use crate::ipc::protocol::DEFAULT_SYSTEM_SOCKET;

/// Garante o daemon system no socket padrão.
/// `on_wait(segundos)` é chamado a cada segundo enquanto aguarda o usuário
/// autorizar no diálogo do polkit (a UI mostra que está esperando).
pub fn ensure_system_daemon<F: FnMut(u32)>(
    timeout: Duration,
    mut on_wait: F,
) -> Result<PathBuf, YuaError> {
    let sock = PathBuf::from(DEFAULT_SYSTEM_SOCKET);
    if YuaClient::connect(&sock).is_ok() {
        return Ok(sock);
    }

    let osd = std::env::current_exe()
        .ok()
        .and_then(|e| e.parent().map(|d| d.join("yua-osd")))
        .filter(|p| p.is_file())
        .or_else(|| which("yua-osd"))
        .or_else(|| Some(PathBuf::from("/usr/local/bin/yua-osd")).filter(|p| p.is_file()))
        .ok_or_else(|| {
            YuaError::new(
                ErrorDomain::Dep,
                3,
                "Binário yua-osd não encontrado ao lado do executável, no PATH nem em /usr/local/bin",
            )
            .with_recommendation("Rode `cargo build --release` na raiz do projeto ou `bash scripts/install-daemon.sh`.")
        })?;

    std::process::Command::new("pkexec")
        .arg(osd)
        .arg("--system")
        .arg("--socket")
        .arg(&sock)
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
        .map_err(|e| {
            YuaError::new(ErrorDomain::Dep, 4, "Não foi possível executar o pkexec")
                .with_technical(e.to_string())
                .with_recommendation("O pkexec (policykit-1) é pré-requisito do modo privilegiado: sudo apt install policykit-1.")
        })?;

    let deadline = Instant::now() + timeout;
    let mut secs = 0u32;
    loop {
        std::thread::sleep(Duration::from_secs(1));
        secs += 1;
        on_wait(secs);
        if YuaClient::connect(&sock).is_ok() {
            return Ok(sock);
        }
        if Instant::now() >= deadline {
            return Err(YuaError::new(
                ErrorDomain::Auth,
                7,
                "O daemon system não apareceu no socket a tempo",
            )
            .with_technical("Sem agente de diálogo polkit e sem TTY, a senha não pode ser pedida (Request dismissed)")
            .with_recommendation("Num TERMINAL seu, rode `yua daemon system` — o prompt de senha aparece ali (funciona sempre). No app, ative o agente: ~/.config/autostart (já instalado) + reiniciar a sessão."));
        }
    }
}
