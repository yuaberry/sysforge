# YUA OS MANAGER

**Plataforma universal de deployment e recuperação de sistemas operacionais.**
Rust + Tauri 2 + React. Linux UEFI nativo. Sem progresso falso, sem dados inventados — cada operação é real e auditável ou é declaradamente indisponível com código de erro.

> **Estado atual:** Fase 1 concluída (fundação + engines de leitura + daemon IPC + CLI de terminal). O fluxo de instalação real (fases 2–9) está em construção — veja `IMPLEMENTATION_STATUS.md` para o que existe DE VERDADE em cada etapa.

---

## Componentes

| Camada | Caminho | O que é |
|---|---|---|
| Núcleo | `core/` | Erros codificados (`YUA-XXX-NNN`), executor com classes de risco, state machine persistida, engines de hardware/disco/boot/UEFI, protocolo IPC |
| Daemon | `daemon/` | `yua-osd` — a única porta privilegiada. Socket unix, `SO_PEERCRED`, auditoria JSONL, fail-closed para destrutivo |
| CLI | `cli/` | `yua` — executável de terminal com efeitos (banner, spinner, tabelas, barras, badges) |
| App desktop | `apps/desktop/` | Tauri 2 + React 18 — leitura in-process; escrita só via daemon |
| Deploy | `deploy/systemd/` | Socket systemd, service com hardening, actions polkit |
| Scripts | `scripts/` | `bootstrap-linux.sh` (deps do ambiente) e `install-daemon.sh` (modo sistema) |

## Início rápido

```bash
# 1. Compilar workspace (core + daemon + CLI) — funciona sem sudo
cargo build --workspace
cargo test --workspace          # 35 testes, inclusive contra o hardware real

# 2. Usar o CLI (release + symlink opcional em ~/.local/bin/yua)
cargo build --release
./target/release/yua doctor     # diagnóstico do ambiente com instruções exatas
./target/release/yua status     # sistema vivo real
./target/release/yua disks      # inventário com serial/modelo reais
./target/release/yua boot       # entradas UEFI reais + ESP

# 3. Daemon em modo dev (somente leitura, fail-closed)
./target/release/yua daemon run          # primeiro plano
./target/release/yua daemon status       # consulta via IPC

# 4. App desktop (requer headers de build — o doctor diz o comando exato)
bash scripts/bootstrap-linux.sh --check  # ver o que falta
bash scripts/bootstrap-linux.sh          # instalar (sudo)
cd apps/desktop && npm install && npm run build
cd src-tauri && cargo build              # compila o yua-desktop
```

## Princípios do projeto

1. **Funcionalidade real > aparência.** Nenhuma tela "de mentira" com botões que não fazem nada.
2. **Validação > suposição.** Todo comando externo passa pelo `Executor` (sem shell, timeout, log).
3. **Log real > progresso falso.** O que não existe é reportado com código (`YUA-DEP-005` etc.).
4. **Fail-closed.** Sem autorização, a resposta é NÃO — não "talvez".
5. **Disco do host é intocável.** Guard por código (`YUA-DISK-010`) + testes destrutivos só em QEMU (Fase 9).

## Documentação

- `docs/ARCHITECTURE.md` — design dos crates, protocolo IPC, state machine
- `docs/SECURITY.md` — modelo de ameaças, classes de risco, hardening, resposta a incidentes
- `docs/DEVELOPMENT.md` — como desenvolver, testar e empacotar
- `IMPLEMENTATION_STATUS.md` — auditoria honesta do que está pronto, em progresso ou bloqueado

## Licença

MIT © Yua Devs
