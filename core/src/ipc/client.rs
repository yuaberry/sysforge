//! Cliente IPC do daemon (usado pelo CLI e, na Fase 2, pela GUI).

use std::io::{BufRead, BufReader, Write};
use std::os::unix::net::UnixStream;
use std::path::{Path, PathBuf};
use std::time::Duration;

use crate::error::{ErrorDomain, SysforgeError};
use crate::ipc::protocol::{Request, Response};

#[derive(Debug, Clone, Copy)]
pub enum IpcTimeout {
    Default,
    Long,
}

impl IpcTimeout {
    fn duration(self) -> Duration {
        match self {
            IpcTimeout::Default => Duration::from_secs(30),
            // Privilegiado: pkcheck pode abrir o diálogo do polkit e esperar
            // até 180s o usuário digitar a senha — o cliente precisa esperar
            // MAIS que isso (bug real: cliente desistia em 30s com EAGAIN
            // antes de o usuário conseguir responder o diálogo).
            IpcTimeout::Long => Duration::from_secs(200),
        }
    }
}

pub struct YuaClient {
    stream: UnixStream,
    reader: BufReader<UnixStream>,
    next_id: u64,
}

impl YuaClient {
    pub fn connect(socket: &Path) -> Result<Self, SysforgeError> {
        let stream = UnixStream::connect(socket).map_err(|e| {
            SysforgeError::new(
                ErrorDomain::Io,
                5,
                "Não foi possível conectar ao daemon sysforge-osd",
            )
            .with_technical(format!("socket {}: {e}", socket.display()))
            .with_recommendation("Verifique se o daemon está ativo (`sysforge daemon status`).")
        })?;
        stream
            .set_read_timeout(Some(IpcTimeout::Default.duration()))
            .ok();
        stream
            .set_write_timeout(Some(Duration::from_secs(10)))
            .ok();
        let reader = BufReader::new(stream.try_clone()?);
        Ok(Self {
            stream,
            reader,
            next_id: 1,
        })
    }

    /// Chama um método e retorna o `result`. Erros viram SysforgeError com código.
    pub fn call(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, SysforgeError> {
        let id = self.next_id;
        self.next_id += 1;
        let req = Request {
            id,
            method: method.to_string(),
            params,
        };
        let line = serde_json::to_string(&req)?;
        writeln!(self.stream, "{line}")?;
        self.stream.flush()?;

        let mut buf = String::new();
        let n = self.reader.read_line(&mut buf)?;
        if n == 0 {
            return Err(SysforgeError::new(
                ErrorDomain::Io,
                6,
                "O daemon encerrou a conexão sem responder",
            ));
        }
        let resp: Response = serde_json::from_str(buf.trim())?;
        if resp.id != id {
            return Err(SysforgeError::new(
                ErrorDomain::Io,
                7,
                "Resposta fora de ordem do protocolo",
            )
            .with_technical(format!("esperado id {id}, recebido {}", resp.id)));
        }
        if resp.ok {
            resp.result
                .ok_or_else(|| SysforgeError::new(ErrorDomain::Io, 8, "Resposta sem resultado"))
        } else {
            Err(resp
                .error
                .map(SysforgeError::from)
                .unwrap_or_else(|| SysforgeError::new(ErrorDomain::Io, 9, "Erro sem detalhes")))
        }
    }

    /// Chama um método que pode abrir o diálogo de autorização do polkit
    /// (privilegiados): usa timeout longo (200s) — o pkcheck espera até 180s
    /// o usuário digitar a senha. Restaura o timeout padrão ao terminar.
    pub fn call_interactive(
        &mut self,
        method: &str,
        params: serde_json::Value,
    ) -> Result<serde_json::Value, SysforgeError> {
        self.stream
            .set_read_timeout(Some(IpcTimeout::Long.duration()))
            .ok();
        let r = self.call(method, params);
        self.stream
            .set_read_timeout(Some(IpcTimeout::Default.duration()))
            .ok();
        r
    }

    /// Echo de sanidade do protocolo.
    pub fn ping(&mut self) -> Result<String, SysforgeError> {
        let v = self.call(
            crate::ipc::protocol::METHOD_ECHO,
            serde_json::json!({"message": "ping"}),
        )?;
        Ok(v.as_str().unwrap_or("pong").to_string())
    }

    /// Caminho de socket resolvido: explícito, dev padrão ou sistema.
    pub fn resolve_socket(explicit: Option<&Path>) -> PathBuf {
        explicit
            .map(|p| p.to_path_buf())
            .unwrap_or_else(crate::ipc::protocol::dev_socket_default)
    }
}
