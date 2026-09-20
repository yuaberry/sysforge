//! `SystemCommandExecutor` — a ÚNICA porta de saída para comandos do sistema.
//!
//! Por que existe: uma ferramenta que particiona discos não pode simplesmente
//! chamar `Command::new("bash -c ...")`. Todo comando aqui é `program + args`
//! (sem shell), com timeout, ambiente determinístico (`LC_ALL=C`) e log.
//!
//! Classes de risco (alinhadas à policy do daemon):
//! - `ReadOnly`   : nunca altera o sistema (lsblk, blkid, efibootmgr leitura).
//! - `LowRisk`    : altera estado reversível (mount/umount, criar entry EFI).
//! - `Destructive`: apaga dados (mkfs, wipefs, parted write, dd, sfdisk).
//!
//! Regras de execução de `Destructive`:
//! 1. Exige `DiskIdentity` do alvo + `operation_id` persistido.
//! 2. Revalida a identidade IMEDIATAMENTE antes da execução.
//! 3. **Host-disk guard**: recusa operação no disco que hospeda o rootfs
//!    vivo (protege o SSD físico do desenvolvedor/técnico por construção).
//! 4. Modo `DryRun` NUNCA executa destrutivo — registra `WOULD_RUN`.
//!
//! Mocks existem apenas em testes. No caminho real, zero mocks.

use std::collections::BTreeMap;
use std::io::Read;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

use crate::disk::identity::DiskIdentity;
use crate::error::{ErrorDomain, YuaError};

/// Classe de risco da operação. Determina autorização no daemon.
#[derive(Debug, Clone, Copy, PartialEq, Eq, serde::Serialize, serde::Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum Risk {
    ReadOnly,
    LowRisk,
    Destructive,
}

impl Risk {
    pub fn label(self) -> &'static str {
        match self {
            Risk::ReadOnly => "READ_ONLY",
            Risk::LowRisk => "LOW_RISK",
            Risk::Destructive => "DESTRUCTIVE",
        }
    }
}

/// Modo de execução. `DryRun` executa read-only de verdade (dados reais
/// para o plano) e registra o que executaria nas demais classes.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum ExecutorMode {
    Real,
    DryRun,
}

/// Especificação imutável de um comando externo.
#[derive(Debug, Clone)]
pub struct CommandSpec {
    pub program: String,
    pub args: Vec<String>,
    pub env: BTreeMap<String, String>,
    pub timeout: Duration,
}

impl CommandSpec {
    pub fn new(program: impl Into<String>) -> Self {
        Self {
            program: program.into(),
            args: Vec::new(),
            env: BTreeMap::new(),
            timeout: Duration::from_secs(30),
        }
    }

    pub fn arg(mut self, a: impl Into<String>) -> Self {
        self.args.push(a.into());
        self
    }

    pub fn args<I: IntoIterator<Item = S>, S: Into<String>>(mut self, it: I) -> Self {
        self.args.extend(it.into_iter().map(Into::into));
        self
    }

    pub fn env(mut self, k: &str, v: &str) -> Self {
        self.env.insert(k.to_string(), v.to_string());
        self
    }

    pub fn timeout(mut self, d: Duration) -> Self {
        self.timeout = d;
        self
    }

    /// Linha de comando para log (escaped, sem shell de verdade envolvido).
    pub fn display(&self) -> String {
        let mut s = self.program.clone();
        for a in &self.args {
            s.push(' ');
            s.push_str(&crate::error::shell_escape(a));
        }
        s
    }

    fn build_command(&self) -> Command {
        let mut c = Command::new(&self.program);
        c.args(&self.args);
        // Ambiente determinístico: parsing estável independente do locale.
        c.env("LC_ALL", "C");
        c.env("LANG", "C");
        for (k, v) in &self.env {
            c.env(k, v);
        }
        c.stdout(Stdio::piped());
        c.stderr(Stdio::piped());
        c
    }
}

/// Resultado da execução (ou simulação) de um comando.
#[derive(Debug, Clone, serde::Serialize)]
pub struct ExecResult {
    pub program: String,
    pub cmdline: String,
    pub exit_code: Option<i32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u128,
    pub risk: Risk,
    /// true quando foi simulado (DryRun) e não executado de fato.
    pub simulated: bool,
    pub timed_out: bool,
}

impl ExecResult {
    pub fn success(&self) -> bool {
        self.exit_code == Some(0)
    }

    pub fn error_if_failed(&self) -> Result<&ExecResult, YuaError> {
        if self.success() {
            Ok(self)
        } else {
            Err(YuaError::command_failed(
                &self.program,
                &[],
                self.exit_code,
                &self.stderr,
            ))
        }
    }
}

/// Executor com modo Real/DryRun e guardas de segurança.
#[derive(Debug, Clone)]
pub struct Executor {
    mode: ExecutorMode,
    /// Apenas o desenvolvimento com QEMU/loop libera destrutivo no disco
    /// do sistema vivo; exige confirmação digitada na UI. Default: false.
    allow_host_disk_destructive: bool,
}

impl Default for Executor {
    fn default() -> Self {
        Self::new(ExecutorMode::Real)
    }
}

impl Executor {
    pub fn new(mode: ExecutorMode) -> Self {
        Self {
            mode,
            allow_host_disk_destructive: false,
        }
    }

    pub fn mode(&self) -> ExecutorMode {
        self.mode
    }

    pub fn allow_host_disk(mut self, allow: bool) -> Self {
        self.allow_host_disk_destructive = allow;
        self
    }

    /// Executa um comando de classe read-only.
    /// Executa SEMPRE (inclusive em DryRun — dados precisam ser reais).
    pub fn run_readonly(&self, spec: CommandSpec) -> Result<ExecResult, YuaError> {
        self.run(spec, Risk::ReadOnly)
    }

    /// Executa um comando de baixo risco (reversível).
    /// Em DryRun apenas registra `WOULD_RUN`.
    pub fn run_low_risk(&self, spec: CommandSpec) -> Result<ExecResult, YuaError> {
        self.run(spec, Risk::LowRisk)
    }

    /// Executa um comando DESTRUTIVO com todas as barreiras:
    /// revalidação de identidade + guard do disco host + operation_id.
    pub fn run_destructive(
        &self,
        spec: CommandSpec,
        identity: &DiskIdentity,
        operation_id: &str,
    ) -> Result<ExecResult, YuaError> {
        if operation_id.trim().is_empty() {
            return Err(YuaError::new(
                ErrorDomain::State,
                1,
                "Operação destrutiva sem operation_id registrado",
            )
            .with_recommendation(
                "Isto é um bug interno: toda operação destrutiva precisa de um operation_id persistido antes de executar.",
            ));
        }

        // Guard: nunca destrutivo no disco que hospeda o rootfs vivo.
        if let Some(host_disk) = host_root_disk() {
            let target_disk = disk_of_partition(&identity.path)
                .unwrap_or_else(|| identity.path.clone());
            if targets_same_disk(&target_disk, &host_disk) && !self.allow_host_disk_destructive
            {
                return Err(YuaError::new(
                    ErrorDomain::Disk,
                    10,
                    "Operação destrutiva recusada: o alvo é o disco do sistema em execução",
                )
                .with_technical(format!(
                    "guard do host: alvo {target_disk} contém o rootfs em {host_disk}"
                ))
                .with_recommendation(
                    "Testes destrutivos devem ocorrer em discos virtuais (QEMU/loop). Para liberar explicitamente em disco físico, use a confirmação digitada na UI com o modo desenvolvedor.",
                ));
            }
        }

        // Revalidação imediata da identidade do alvo.
        // Em DryRun, falha é registrada (o plano exibe o problema);
        // em Real, ela ABORTA a operação — fail-closed.
        if let Err(e) = identity.revalidate_now() {
            if self.mode == ExecutorMode::Real {
                tracing::error!(code = %e.code, disk = %identity.path, "identity revalidation failed before destructive op");
                return Err(e);
            }
            tracing::warn!(code = %e.code, disk = %identity.path, "dry-run: identity revalidation failed (real mode would abort)");
        }

        self.run(spec, Risk::Destructive)
    }

    fn run(&self, spec: CommandSpec, risk: Risk) -> Result<ExecResult, YuaError> {
        let started = Instant::now();

        // Dry-run: read-only executa de verdade; o resto é apenas registrado.
        if self.mode == ExecutorMode::DryRun && risk != Risk::ReadOnly {
            tracing::info!(risk = risk.label(), "WOULD_RUN: {}", spec.display());
            return Ok(ExecResult {
                program: spec.program.clone(),
                cmdline: spec.display(),
                exit_code: Some(0),
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: started.elapsed().as_millis(),
                risk,
                simulated: true,
                timed_out: false,
            });
        }

        tracing::debug!(risk = risk.label(), "EXEC: {}", spec.display());
        let mut child = spec.build_command().spawn().map_err(|e| {
            YuaError::new(
                ErrorDomain::Dep,
                2,
                format!("Não foi possível executar `{}`", spec.program),
            )
            .with_technical(e.to_string())
            .with_recommendation("Verifique se o binário existe (`yua doctor` verifica dependências).")
        })?;

        // Timeout via polling (sem crates extras).
        let deadline = Instant::now() + spec.timeout;
        loop {
            match child.try_wait() {
                Ok(Some(_)) => break,
                Ok(None) => {
                    if Instant::now() >= deadline {
                        let _ = child.kill();
                        let _ = child.wait();
                        return Err(YuaError::new(
                            ErrorDomain::Io,
                            4,
                            format!("`{}` excedeu o tempo limite de {:?}s", spec.program, spec.timeout.as_secs()),
                        )
                        .with_recommendation("Aumente o timeout nas configurações ou investigue o travamento da ferramenta."));
                    }
                    std::thread::sleep(Duration::from_millis(10));
                }
                Err(e) => return Err(e.into()),
            }
        }

        let mut stdout = String::new();
        let mut stderr = String::new();
        if let Some(mut s) = child.stdout.take() {
            let _ = s.read_to_string(&mut stdout);
        }
        if let Some(mut s) = child.stderr.take() {
            let _ = s.read_to_string(&mut stderr);
        }
        let status = child.wait()?;

        let result = ExecResult {
            program: spec.program.clone(),
            cmdline: spec.display(),
            exit_code: status.code(),
            stdout,
            stderr,
            duration_ms: started.elapsed().as_millis(),
            risk,
            simulated: false,
            timed_out: false,
        };

        if result.success() {
            tracing::debug!(risk = risk.label(), ms = result.duration_ms, "OK: {}", spec.program);
        } else {
            tracing::warn!(risk = risk.label(), exit = ?result.exit_code, "FAIL: {} :: {}", spec.program, result.stderr.trim());
        }
        Ok(result)
    }
}

/// Descobre qual disco hospeda o rootfs vivo ("/" real, não em container).
/// Lê /proc/mounts — sem root, determinístico.
pub fn host_root_disk() -> Option<String> {
    let mounts = std::fs::read_to_string("/proc/mounts").ok()?;
    for line in mounts.lines() {
        let mut parts = line.split_whitespace();
        let (dev, mnt, fstype) = (parts.next()?, parts.next()?, parts.next()?);
        if mnt == "/" && fstype != "overlay" && fstype != "squashfs" {
            return Some(dev.to_string());
        }
    }
    None
}

/// "/dev/sda2" -> "/dev/sda"; "/dev/nvme0n1p2" -> "/dev/nvme0n1";
/// "/dev/sda" -> "/dev/sda".
pub fn disk_of_partition(dev: &str) -> Option<String> {
    let p = Path::new(dev);
    let name = p.file_name()?.to_str()?.to_string();
    let stem = p.parent()?;
    let base = if let Some(stripped) = name.strip_suffix(|c: char| c.is_ascii_digit()) {
        // nvme0n1p2 -> nvme0n1p -> nvme0n1 (o loop trata o 'p' final)
        if let Some(s2) = stripped.strip_suffix('p') {
            if s2.chars().last().map(|c| c.is_ascii_digit()).unwrap_or(false) {
                s2.to_string()
            } else {
                stripped.to_string()
            }
        } else {
            stripped.to_string()
        }
    } else {
        name
    };
    Some(stem.join(base).to_string_lossy().to_string())
}

/// Compara dois caminhos de dispositivo reduzindo ambos ao disco-base.
fn targets_same_disk(a: &str, b: &str) -> bool {
    let norm = |s: &str| s.trim_start_matches("/dev/").to_string();
    let (na, nb) = (norm(a), norm(b));
    let da = disk_of_partition(&na).unwrap_or(na);
    let db = disk_of_partition(&nb).unwrap_or(nb);
    da == db
}

/// Localiza binário no PATH sem shell (which real, em Rust).
pub fn which(program: &str) -> Option<PathBuf> {
    if program.contains('/') {
        let p = PathBuf::from(program);
        return p.is_file().then_some(p);
    }
    let path = std::env::var_os("PATH")?;
    std::env::split_paths(&path)
        .map(|dir| dir.join(program))
        .find(|p| p.is_file())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn dry_run_executes_readonly_but_not_destructive() {
        let ex = Executor::new(ExecutorMode::DryRun);

        // Read-only roda de verdade em DryRun (dados reais para o plano).
        let r = ex
            .run_readonly(CommandSpec::new("sh").arg("-c").arg("echo real"))
            .unwrap();
        assert!(!r.simulated);
        assert_eq!(r.stdout.trim(), "real");

        // Destrutivo em DryRun NÃO executa.
        let r = ex
            .run_destructive(
                CommandSpec::new("sh").arg("-c").arg("echo destroyed > /tmp/yua-should-not-exist"),
                &DiskIdentity::synthetic_for_tests("/dev/zzz9"),
                "op-test-1",
            )
            .unwrap();
        assert!(r.simulated);
        assert!(!Path::new("/tmp/yua-should-not-exist").exists());
    }

    #[test]
    fn host_disk_guard_blocks_rootfs_disk() {
        let ex = Executor::new(ExecutorMode::Real);
        let host = host_root_disk().expect("test needs /proc/mounts");
        let host_disk = disk_of_partition(&host).unwrap_or(host);
        let identity = DiskIdentity::synthetic_for_tests(&host_disk);

        let err = ex
            .run_destructive(
                CommandSpec::new("true"),
                &identity,
                "op-guard-test",
            )
            .unwrap_err();
        assert_eq!(err.code, "YUA-DISK-010", "deve recusar destrutivo no disco do rootfs vivo");
    }

    #[test]
    fn partition_to_disk_mapping() {
        assert_eq!(disk_of_partition("/dev/sda2").as_deref(), Some("/dev/sda"));
        assert_eq!(disk_of_partition("/dev/nvme0n1p3").as_deref(), Some("/dev/nvme0n1"));
        assert_eq!(disk_of_partition("/dev/sda").as_deref(), Some("/dev/sda"));
    }

    #[test]
    fn destructive_requires_operation_id() {
        let ex = Executor::new(ExecutorMode::Real);
        let err = ex
            .run_destructive(
                CommandSpec::new("true"),
                &DiskIdentity::synthetic_for_tests("/dev/null"),
                "",
            )
            .unwrap_err();
        assert_eq!(err.code, "YUA-STATE-001");
    }
}
