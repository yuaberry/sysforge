# Arquitetura — YUA OS MANAGER

## Visão de alto nível

```
┌─────────────┐   IPC NDJSON    ┌────────────┐
│  yua (CLI)  │────────────────▶│  yua-osd   │──▶ leitura: root quando precisar
│  app desktop│  (unix socket)  │ (daemon)   │──▶ escrita: Fase 2+ com polkit
└──────┬──────┘                 └─────┬──────┘
       │  leitura IN-PROCESS          │
       ▼                               ▼
   ┌─────────────────────────────────────┐
   │              yua-core               │
   │  error · executor · state ·         │
   │  hw/system · disk/* · boot/* ·      │
   │  capability · ipc/protocol           │
   └─────────────────────────────────────┘
```

**Regra de ouro:** GUI e CLI fazem leitura in-process (sem daemon, sem latência).
Qualquer operação privilegiada (escrita) passa EXCLUSIVAMENTE pelo `yua-osd`.

## yua-core

### Erros codificados (`error.rs`)
Todo erro carrega: código estável (`YUA-DISK-001`), mensagem legível, detalhe técnico e ação recomendada. Domínios: `DISK, BOOT, IMAGE, NET, UEFI, WIN, LINUX, DEP, AUTH, STATE, IO, NOTSUP`.

### Executor (`executor.rs`)
A única porta para comandos externos:
- `program + args` — **nunca** string de shell (anti-injection);
- ambiente determinístico (`LC_ALL=C`) e timeout via polling;
- classes de risco: `ReadOnly` (sempre roda), `LowRisk` (reversível), `Destructive`;
- modos: `Real` e `DryRun` (read-only executa de verdade; destrutivo apenas loga `WOULD_RUN`);
- `run_destructive()` exige: `operation_id` persistido + **revalidação de identidade** na hora + **host-disk guard** (`/proc/mounts` → disco do rootfs vivo é recusado com `YUA-DISK-010`).

### State machine (`state.rs`)
Fluxo: `IDLE → PLANNING → VALIDATING → STAGING → READY → REBOOTING → BOOTED_RECOVERY → DEPLOYING → CONFIGURING → VERIFYING → COMPLETED`, com saídas `FAILED/CANCELLED/ROLLBACK` de qualquer estado ativo.
Persistência atômica (tmp + fsync + rename) com checksum SHA-256 do próprio conteúdo — edição manual é detectada (`YUA-STATE-003`).

### Engines de leitura
| Engine | Fonte de dados | Observação |
|---|---|---|
| `hw/system` | `/etc/os-release`, `/proc/*`, `/sys/*`, efivarfs | Secure Boot = byte no offset 4 do efivar |
| `disk/lsblk` | `lsblk --json -b` (colunas fixas) | parse puro testável contra fixture real |
| `disk/udev` | `/run/udev/data/b{maj}:{min}` | serial/model sem root |
| `disk/blkid` | `blkid -o export` | filesystem/UUID |
| `disk/smart` | `smartctl --json` | **honesto**: sem binário/sem root → `YUA-DEP-005/006` |
| `disk/identity` | lsblk + udev | revalidação anti-hot-plug (`YUA-DISK-009`) |
| `boot/efi` | `efibootmgr` | parser testado contra saída real |
| `boot/esp` | `/proc/mounts` + `statvfs` | detecta permissão restrita (umask=0077) |

## Protocolo IPC (v1)

NDJSON sobre unix socket. `Request {id, method, params}` / `Response {id, ok, result|error{code,message,...}}`.

Métodos publicados (todos read-only): `v1.echo`, `v1.system.info`, `v1.disks.list`, `v1.efi.entries`, `v1.capabilities`, `v1.daemon.info`.

O registro `DESTRUCTIVE_REGISTRY` (`v1.disk.wipe`, `v1.disk.format`, `v1.deploy.start`, `v1.boot.set_next`) é recusado em TODOS os modos na Fase 1: modo dev → `YUA-AUTH-002`; modo system → `YUA-AUTH-004`. Isso é fail-closed testável (ver `daemon/tests/ipc_dev.rs`).

### Modos do daemon
- **dev** (`yua-osd --dev`): socket em `$XDG_RUNTIME_DIR`, chmod 0600, só o próprio uid conecta; destrutivo recusado.
- **system** (systemd): `/run/yua-osd.sock` (0666), euid 0; auth por método via `pkcheck` na Fase 2; auditoria em `/var/lib/yua-os-manager/audit.jsonl`.

## App desktop

Tauri 2 (`yua-desktop`, identificador `dev.yua.osmanager`), frontend React 18 + Vite 5 + TS 5. Comandos Tauri chamam `yua-core` in-process para leitura. Páginas ainda não implementadas mostram estado honesto "planejado — fase N" com a razão no log; nunca stubs falsos.

## Decisões técnicas registradas

- **sem zbus**: pkcheck via subprocess (menos dependência, mesmo resultado);
- **sem indicatif**: spinner manual no CLI (30 linhas, zero deps);
- **Vite 5** (não 7): Node 18.19 do Mint 22.3;
- **workspace exclui `apps/desktop/src-tauri`**: permite `cargo build --workspace` sem os headers webkit (o doctor instrui a instalação);
- **efibootmgr como parser, não lib**: saída é estável e o teste usa a saída REAL desta máquina.
