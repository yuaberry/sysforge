//! Sistema de erros do YUA com códigos rastreáveis.
//!
//! TODO erro carrega: código estável (`YUA-XXX-NNN`), mensagem legível para o
//! usuário, detalhe técnico e ação recomendada. A UI nunca mostra só
//! "something went wrong".

use std::fmt;

/// Categorias de erro do YUA. Os códigos são estáveis e documentados em
/// docs/TROUBLESHOOTING.md.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum ErrorDomain {
    Disk,
    Boot,
    Image,
    Net,
    Uefi,
    Windows,
    Linux,
    Dep,
    Auth,
    State,
    Io,
    NotSupported,
}

impl ErrorDomain {
    pub fn prefix(self) -> &'static str {
        match self {
            ErrorDomain::Disk => "YUA-DISK",
            ErrorDomain::Boot => "YUA-BOOT",
            ErrorDomain::Image => "YUA-IMAGE",
            ErrorDomain::Net => "YUA-NET",
            ErrorDomain::Uefi => "YUA-UEFI",
            ErrorDomain::Windows => "YUA-WIN",
            ErrorDomain::Linux => "YUA-LINUX",
            ErrorDomain::Dep => "YUA-DEP",
            ErrorDomain::Auth => "YUA-AUTH",
            ErrorDomain::State => "YUA-STATE",
            ErrorDomain::Io => "YUA-IO",
            ErrorDomain::NotSupported => "YUA-NOTSUP",
        }
    }
}

/// Erro estruturado do YUA.
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error, serde::Serialize, serde::Deserialize)]
pub struct YuaError {
    /// Código estável, ex.: "YUA-DISK-001".
    pub code: String,
    /// Mensagem legível para o usuário (o que aconteceu).
    pub message: String,
    /// Detalhe técnico (por que aconteceu).
    pub technical: String,
    /// Ação recomendada (o que fazer).
    pub recommendation: String,
}

impl YuaError {
    pub fn new(domain: ErrorDomain, num: u32, message: impl Into<String>) -> Self {
        Self {
            code: format!("{}-{:03}", domain.prefix(), num),
            message: message.into(),
            technical: String::new(),
            recommendation: String::new(),
        }
    }

    pub fn with_technical(mut self, tech: impl Into<String>) -> Self {
        self.technical = tech.into();
        self
    }

    pub fn with_recommendation(mut self, rec: impl Into<String>) -> Self {
        self.recommendation = rec.into();
        self
    }

    /// Erro de comando externo com saída capturada.
    pub fn command_failed(program: &str, args: &[String], exit: Option<i32>, stderr: &str) -> Self {
        let args_joined = args
            .iter()
            .map(|a| shell_escape(a))
            .collect::<Vec<_>>()
            .join(" ");
        Self::new(
            ErrorDomain::Io,
            1,
            format!("O comando `{program}` falhou"),
        )
        .with_technical(format!(
            "`{program} {args_joined}` terminou com status {exit:?}. stderr: {}",
            stderr.trim()
        ))
        .with_recommendation(
            "Verifique se a ferramenta está instalada e se o usuário tem permissão. Consulte `yua doctor`.",
        )
    }
}

impl fmt::Display for YuaError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "[{}] {}", self.code, self.message)?;
        if !self.technical.is_empty() {
            write!(f, "\n  detalhe: {}", self.technical)?;
        }
        if !self.recommendation.is_empty() {
            write!(f, "\n  sugestão: {}", self.recommendation)?;
        }
        Ok(())
    }
}

impl From<std::io::Error> for YuaError {
    fn from(e: std::io::Error) -> Self {
        Self::new(ErrorDomain::Io, 2, format!("Erro de E/S: {e}"))
            .with_technical(e.to_string())
            .with_recommendation("Verifique permissões de arquivos/diretórios e espaço em disco.")
    }
}

impl From<serde_json::Error> for YuaError {
    fn from(e: serde_json::Error) -> Self {
        Self::new(ErrorDomain::Io, 3, format!("Falha ao processar JSON: {e}"))
            .with_technical(e.to_string())
            .with_recommendation("Isto geralmente indica saída inesperada de uma ferramenta do sistema. Reporte com `yua logs`.")
    }
}

/// Escapa um argumento para exibição em log (não usamos shell; isto é só
/// para tornar o log não-ambíguo).
pub fn shell_escape(arg: &str) -> String {
    if arg.is_empty() || arg.chars().any(|c| c.is_whitespace() || c == '"') {
        format!("{:?}", arg)
    } else {
        arg.to_string()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn error_codes_format() {
        let e = YuaError::new(ErrorDomain::Disk, 1, "disco não encontrado");
        assert_eq!(e.code, "YUA-DISK-001");
        let e2 = YuaError::new(ErrorDomain::Uefi, 12, "x");
        assert_eq!(e2.code, "YUA-UEFI-012");
    }

    #[test]
    fn display_includes_recommendation() {
        let e = YuaError::new(ErrorDomain::Auth, 2, "autorização necessária")
            .with_technical("polkit indisponível")
            .with_recommendation("instale policykit-1");
        let s = e.to_string();
        assert!(s.contains("YUA-AUTH-002"));
        assert!(s.contains("detalhe: polkit indisponível"));
        assert!(s.contains("sugestão: instale policykit-1"));
    }
}
