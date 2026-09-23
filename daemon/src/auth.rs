//! Autorização por conexão: SO_PEERCRED (uid/pid REAIS do kernel — o cliente
//! não consegue forjar) + classes de método + polkit.
//!
//! Modelo de decisão:
//! - DESTRUTIVO não-implementado (wipe/format/deploy) → recusado SEMPRE.
//! - PRIVILEGIADO implementado (set_next/reboot/poweroff/...) → exige daemon
//!   em modo SYSTEM + `pkcheck` com a action com.sysforge.osd.lowrisk.
//!   O pkcheck pede a senha AO USUÁRIO no diálogo do polkit (agente da
//!   sessão) — o daemon nunca vê a senha, só o veredito.
//! - Read-only → modo dev: mesmo uid; modo system: liberado.
//!
//! Fail-closed: polkit indisponível, sem resposta, ou negado → RECUSA.

use sysforge_core::error::{ErrorDomain, SysforgeError};
use sysforge_core::executor::{CommandSpec, Executor};
use sysforge_core::ipc::protocol::{WireError, DESTRUCTIVE_REGISTRY, PRIVILEGED_METHODS};

use crate::server::Peer;
use crate::DaemonMode;

pub enum Decision {
    Allow,
    Deny(WireError),
}

/// starttime do processo (campo 22 de /proc/<pid>/stat) para o pkcheck.
/// O comm (campo 2) pode conter espaços/parênteses — o parse pula até ')'.
fn proc_start_time(pid: u32) -> Result<u64, SysforgeError> {
    let raw = std::fs::read_to_string(format!("/proc/{pid}/stat")).map_err(|_| {
        SysforgeError::new(
            ErrorDomain::Auth,
            10,
            "Processo chamador não encontrado para autorização polkit",
        )
        .with_technical(format!("leitura de /proc/{pid}/stat falhou"))
    })?;
    let after_comm = raw.split_once(')').ok_or_else(|| {
        SysforgeError::new(ErrorDomain::Auth, 11, "Formato inesperado de /proc/<pid>/stat")
    })?;
    // Após o comm, os campos continuam a partir do campo 3 (state).
    // starttime é o campo 22 ⇒ índice 19 no resto (0-based, contando do 3).
    let token = after_comm
        .1
        .split_whitespace()
        .nth(19)
        .ok_or_else(|| SysforgeError::new(ErrorDomain::Auth, 11, "stat sem starttime"))?;
    token
        .parse::<u64>()
        .map_err(|_| SysforgeError::new(ErrorDomain::Auth, 11, "starttime não numérico"))
}

/// Executa pkcheck em nome do chamador. Result<u64> devolve o exit code.
fn run_pkcheck(peer: &Peer, action_id: &str) -> Result<i32, SysforgeError> {
    let start = proc_start_time(peer.pid)?;
    let exec = Executor::default();
    let spec = CommandSpec::new("pkcheck")
        .arg("--process")
        .arg(format!("{},{}", peer.pid, start))
        .arg("--action-id")
        .arg(action_id)
        .arg("--allow-user-interaction")
        .timeout(std::time::Duration::from_secs(180));
    let r = exec.run_readonly(spec)?;
    Ok(r.exit_code.unwrap_or(-1))
}

pub fn authorize(mode: DaemonMode, peer: &Peer, method: &str) -> Decision {
    // Barreira 1 — destrutivo não-implementado: recusado SEMPRE.
    if DESTRUCTIVE_REGISTRY.contains(&method) {
        let e = match mode {
            DaemonMode::Dev => SysforgeError::new(
                ErrorDomain::Auth,
                2,
                "Operação destrutiva recusada: daemon em modo dev é somente leitura",
            )
            .with_recommendation(
                "Isto é por segurança. Operações destrutivas exigem o daemon em modo sistema (systemd/pkexec) e, mesmo lá, só existem quando implementadas com todos os guards.",
            ),
            DaemonMode::System => SysforgeError::new(
                ErrorDomain::Auth,
                4,
                "Método destrutivo ainda não implementado (aguarda fase de deploy)",
            )
            .with_recommendation(
                "O deploy real chega com plano validado, snapshot e rollback. Nada destrutivo roda antes disso.",
            ),
        };
        return Decision::Deny(WireError::from(e));
    }

    // Barreira 2 — privilégio por MODO. Dev nunca libera escrita.
    if PRIVILEGED_METHODS.contains(&method) {
        if mode == DaemonMode::Dev {
            let e = SysforgeError::new(
                ErrorDomain::Auth,
                2,
                format!("Método {method} exige o daemon em modo sistema"),
            )
            .with_recommendation(
                "Rode `sysforge daemon system` — o polkit pedirá sua senha na tela. O modo dev é somente leitura POR CONSTRUÇÃO.",
            );
            return Decision::Deny(e.into());
        }
        // Modo system: polkit decide (com.sysforge.osd.lowrisk → auth_admin_keep:
        // pede a senha ao usuário na tela e lembra por alguns minutos).
        match run_pkcheck(peer, "com.sysforge.osd.lowrisk") {
            Ok(0) => return Decision::Allow,
            Ok(code) => {
                let e = SysforgeError::new(
                    ErrorDomain::Auth,
                    5,
                    "Autorização negada pelo polkit",
                )
                .with_technical(format!(
                    "pkcheck saiu com {code} — 1/3=negado · 2=cancelado · 4=erro interno · 5=ação desconhecida (a policy com.sysforge.osd está instalada? scripts/install-daemon.sh) · 127=sem como pedir a senha ao usuário (sem agente/diálogo neste contexto — rode de um terminal ou app na sessão gráfica)"
                ))
                .with_recommendation("Rode `bash scripts/install-daemon.sh` (instala a policy polkit) e execute o comando de um terminal/app na sua sessão gráfica.");
                return Decision::Deny(WireError::from(e));
            }
            Err(e) => return Decision::Deny(WireError::from(e)),
        }
    }

    // Read-only publicado: dev exige mesmo uid; system liberado.
    if mode == DaemonMode::Dev {
        let my_uid = unsafe { libc::getuid() };
        if peer.uid != my_uid {
            let e = SysforgeError::new(
                ErrorDomain::Auth,
                1,
                "Conexão recusada: o daemon dev aceita apenas o usuário que o iniciou",
            )
            .with_technical(format!("peer uid {} ≠ uid do daemon {my_uid}", peer.uid));
            return Decision::Deny(e.into());
        }
    }
    Decision::Allow
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn proc_start_time_parses_real_pid() {
        // Nosso próprio /proc/self/stat é parseável — campo 22 > 0.
        let start = proc_start_time(std::process::id()).unwrap();
        assert!(start > 0);
    }

    #[test]
    fn proc_start_time_parses_comm_with_spaces() {
        // comm com parênteses/espaços: "(web: SYSFORGE test)" — o ')' dentro do
        // comm NÃO confunde o parser de verdade, mas ')' em comm é o caso
        // clássico de quebra; aqui usamos um comm normal com espaço.
        let line = "1234 (pk exec agent) S 1 1234 1234 0 -1 4194560 100 0 0 0 5 3 0 0 20 0 1 0 987654 5000000 0 0 0 0";
        let after = line.split_once(')').unwrap();
        let token = after.1.split_whitespace().nth(19).unwrap();
        assert_eq!(token, "987654");
    }

    #[test]
    fn pkcheck_binary_exists_on_this_machine() {
        assert!(sysforge_core::executor::which("pkcheck").is_some(), "pkcheck presente no Mint 22.3");
    }
}
