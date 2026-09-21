# Desenvolvimento — YUA OS MANAGER

## Requisitos

- Rust 1.77+ (testado com 1.98)
- Node 18+ (frontend: Vite 5, React 18, TS 5)
- Linux UEFI (testado: Linux Mint 22.3, kernel 7.0, util-linux 2.39)

## Comandos do dia a dia

```bash
cargo build --workspace          # core + daemon + CLI (sem deps de GUI)
cargo test --workspace           # 35 testes: unit + integração IPC + fixtures reais
cargo build --release            # binários finais em target/release/

./target/release/yua doctor      # ver o que o ambiente tem/falta (com comando exato)

# frontend
cd apps/desktop
npm install
npm run build                    # tsc + vite build (valida TypeScript)
npm run dev                      # dev server (UI abre com BridgeUnavailable honesto fora do Tauri)

# app desktop completo (após bootstrap-linux.sh instalar os headers)
cd apps/desktop/src-tauri && cargo build    # yua-desktop
# ou, da raiz do app: npx tauri dev / npx tauri build
```

## Estrutura de testes

- **Unit** (em cada módulo): parsers puros recebem fixtures EMBUTIDAS (recorte real da saída de lsblk/efibootmgr/udev desta máquina).
- **Ao vivo** (marcados `live_`): rodam contra o hardware real (read-only) — se o disco mudar, o teste falha honesto.
- **Integração** (`daemon/tests/ipc_dev.rs`): sobe o daemon real em dev, conecta o cliente, valida protocolo E o fail-closed (`YUA-AUTH-002`).
- **Futuro (Fase 9)**: suite destrutiva real, exclusivamente contra discos QEMU — CI local com `tests/qemu/`.

## Convenções

- Erro novo? Ganha código estável `YUA-XXX-NNN` + recomendação acionável.
- Comando externo novo? Só entra via `CommandSpec` no `Executor`, com classe de risco definida.
- Estado novo no fluxo? Adicione ao grafo em `state.rs` + teste de transição.
- Funcionalidade não implementada? **Nunca** retorna vazio: código + motivo (UI mostra isso).

## Onde as coisas ficam

| Tipo | Caminho |
|---|---|
| Log do app/CLI | `~/.local/share/yua-os-manager/logs/yua.log` |
| Log do daemon (dev) | `~/.local/share/yua-os-manager/logs/daemon.log` |
| Auditoria (dev) | `~/.local/share/yua-os-manager/logs/audit.jsonl` |
| Auditoria (system) | `/var/lib/yua-os-manager/audit.jsonl` |
| Operações | `/var/lib/yua-os-manager/operations/<id>/operation.json` |
| Socket dev | `$XDG_RUNTIME_DIR/yua-osd.dev.sock` |
| Socket system | `/run/yua-osd.sock` |

## Deploy em modo sistema

```bash
cargo build --release
bash scripts/install-daemon.sh   # copia binários, units, policy; ativa socket
yua daemon status                # deve responder modo "system"
```

## Repositório

Privado: `github.com/yuaberry/yua-os-manager` (main).

## Notas de campo: polkit, TTY e diálogos (aprendido em produção)

- **CLI em terminal funciona SEMPRE**: `pkexec` sem agente gráfico cai no
  `pkttyagent` — prompt de texto no próprio terminal do usuário.
- **App desktop (sem TTY) precisa de um AGENTE de diálogo** (ex.: MATE
  authentication agent) registrado na sessão. Sem ele, pedidos são descartados
  ("Request dismissed") — o doctor tem um check dedicado que detecta isso.
- O YUA instala `~/.config/autostart/polkit-mate-authentication-agent-1.desktop`
  quando o ambiente não tem agente — reinicie a sessão para ativar.
- Contextos sem TTY e sem sessão gráfica (SSH puro, CI, agentes de código)
  NÃO conseguem autenticar por design — fail-closed correto: privilégio passa
  pelo teclado do usuário, não pelo shell de um bot.
