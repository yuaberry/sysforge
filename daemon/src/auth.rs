//! Autorização por conexão: SO_PEERCRED (uid/pid REAIS do kernel — o cliente
//! não consegue forjar) + modo do daemon.
//!
//! Fase 1 (atual): todos os métodos publicados são read-only; o registro
//! DESTRUCTIVE_REGISTRY é recusado em TODOS os modos — fail-closed testável.
//! Fase 2: modo system passa a exigir pkcheck (polkit) por classe de risco:
//!   com.yua.osd.readonly    → allow_active
//!   com.yua.osd.lowrisk     → auth_admin_keep
//!   com.yua.osd.destructive → auth_admin

use yua_core::error::{ErrorDomain, YuaError};
use yua_core::ipc::protocol::{WireError, DESTRUCTIVE_REGISTRY};

use crate::DaemonMode;

pub enum Decision {
    Allow,
    Deny(WireError),
}

pub fn authorize(mode: DaemonMode, peer_uid: u32, method: &str) -> Decision {
    // Barreira 1 — registro destrutivo: recusado SEMPRE na Fase 1.
    if DESTRUCTIVE_REGISTRY.contains(&method) {
        let e = match mode {
            DaemonMode::Dev => YuaError::new(
                ErrorDomain::Auth,
                2,
                "Operação destrutiva recusada: daemon em modo dev é somente leitura",
            )
            .with_technical(format!(
                "método {method} está no registro destrutivo; modo dev nunca libera escrita"
            ))
            .with_recommendation(
                "Isto é por segurança. Para operações destrutivas use o daemon em modo sistema (systemd) — e ainda assim só após a Fase 2 implementá-las.",
            ),
            DaemonMode::System => YuaError::new(
                ErrorDomain::Auth,
                4,
                "Método destrutivo ainda não implementado (Fase 1 = somente leitura)",
            )
            .with_recommendation(
                "O planejamento de instalação real chega no Milestone 1 (UKI + BootNext). Nada de destrutivo roda antes disso.",
            ),
        };
        return Decision::Deny(e.into());
    }

    // Barreira 2 — modo dev: somente o próprio usuário.
    if mode == DaemonMode::Dev {
        let my_uid = unsafe { libc::getuid() };
        if peer_uid != my_uid {
            let e = YuaError::new(
                ErrorDomain::Auth,
                1,
                "Conexão recusada: o daemon dev aceita apenas o usuário que o iniciou",
            )
            .with_technical(format!("peer uid {peer_uid} ≠ uid do daemon {my_uid}"));
            return Decision::Deny(e.into());
        }
    }

    // Métodos read-only publicados: liberados (system mode: qualquer usuário
    // local conecta; autenticação polkit por método entra na Fase 2 junto
    // com os métodos de escrita).
    Decision::Allow
}
