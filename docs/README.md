# SYSFORGE

**Plataforma universal de deployment e recuperação de sistemas operacionais.**
Rust + Tauri 2 + React. Linux UEFI nativo. Sem progresso falso, sem dados inventados — cada operação é real e auditável ou é declaradamente indisponível com código de erro.

> **Estado atual:** Fase 1 concluída (fundação + engines de leitura + daemon IPC + CLI de terminal). O fluxo de instalação real (fases 2–9) está em construção — veja `IMPLEMENTATION_STATUS.md` para o que existe DE VERDADE em cada etapa.

---

## Componentes

| Camada | Caminho | O que é |
|---|---|---|
| Núcleo | `core/` | Erros codificados, executor com classes de risco + host-disk guard, state machine persistida, engines de hardware/disco/boot/UEFI, **controle de energia + reboot-direto-na-BIOS (OsIndications)**, **fluxo Windows 11 (checklist real, autounattend, mídia/Ventoy)**, protocolo IPC |
| Daemon | `daemon/` | `sysforge-osd` — a única porta privilegiada. SO_PEERCRED, autorização polkit via **pkcheck** (diálogo de senha na tela), auditoria JSONL, fail-closed para destrutivo |
| CLI | `cli/` | `sysforge` — terminal com efeitos: `status`, `disks`, `boot next/firmware/remove`, `power`, `install`, `doctor`, `daemon system` |
| App desktop | `apps/desktop/` | Tauri 2 + React 18 — leitura in-process; ações privilegiadas via proxy ao daemon; página Windows 11 com controles reais |
| Deploy | `deploy/systemd/` | Socket systemd, service com hardening, actions polkit com.sysforge.osd.* |
| Scripts | `scripts/` | `bootstrap-linux.sh` (deps, com --check) e `install-daemon.sh` (modo sistema) |

## Fluxo Windows 11 (real, ponta a ponta)

```bash
sysforge install                    # checklist honesto: UEFI, TPM/bypass, ISO, USB, Ventoy, daemon
# conecte o pendrive Ventoy e coloque a ISO em ~/Downloads, depois:
sysforge daemon system               # privilégio via polkit — sua senha na tela
sysforge install --apply            # gera autounattend.xml (com bypass LabConfig) → copia ISO p/ Ventoy
sysforge boot next                   # BootNext one-shot para a entrada USB do firmware (BootOrder intacto)
sysforge power reboot --confirm      # o pendrive assume com o instalador do Windows 11
```

Limites físicos declarados: **após o reboot quem executa é o instalador do Windows** (com as respostas do autounattend). `sysforge boot firmware` reinicia direto na tela do BIOS via `OsIndications` — mecanismo UEFI oficial.

## Privilégio sem senha solta

Nada de sudo espalhado: o CLI/app sobem o daemon via `pkexec`, o **polkit pergunta a sua senha na tela** e o daemon só vê o veredito. Métodos de efeito real exigem `confirm:true` + snapshot prévio do estado UEFI.

## Início rápido

```bash
# 1. Compilar workspace (core + daemon + CLI) — funciona sem sudo
cargo build --workspace
cargo test --workspace          # 35 testes, inclusive contra o hardware real

# 2. Usar o CLI (release + symlink opcional em ~/.local/bin/sysforge)
cargo build --release
./target/release/sysforge doctor     # diagnóstico do ambiente com instruções exatas
./target/release/sysforge status     # sistema vivo real
./target/release/sysforge disks      # inventário com serial/modelo reais
./target/release/sysforge boot       # entradas UEFI reais + ESP

# 3. Daemon em modo dev (somente leitura, fail-closed)
./target/release/sysforge daemon run          # primeiro plano
./target/release/sysforge daemon status       # consulta via IPC

# 4. App desktop (requer headers de build — o doctor diz o comando exato)
bash scripts/bootstrap-linux.sh --check  # ver o que falta
bash scripts/bootstrap-linux.sh          # instalar (sudo)
cd apps/desktop && npm install && npm run build
cd src-tauri && cargo build              # compila o sysforge-desktop
```

## Princípios do projeto

1. **Funcionalidade real > aparência.** Nenhuma tela "de mentira" com botões que não fazem nada.
2. **Validação > suposição.** Todo comando externo passa pelo `Executor` (sem shell, timeout, log).
3. **Log real > progresso falso.** O que não existe é reportado com código (`SF-DEP-005` etc.).
4. **Fail-closed.** Sem autorização, a resposta é NÃO — não "talvez".
5. **Disco do host é intocável.** Guard por código (`SF-DISK-010`) + testes destrutivos só em QEMU (Fase 9).

## Documentação

- `docs/ARCHITECTURE.md` — design dos crates, protocolo IPC, state machine
- `docs/SECURITY.md` — modelo de ameaças, classes de risco, hardening, resposta a incidentes
- `docs/DEVELOPMENT.md` — como desenvolver, testar e empacotar
- `IMPLEMENTATION_STATUS.md` — auditoria honesta do que está pronto, em progresso ou bloqueado

## Licença

MIT © Yua Devs
