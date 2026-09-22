# YUA OS MANAGER

> Gerenciador de sistema desktop (Linux) com **fluxo completo de migração para o Windows 11** — do diagnóstico à BIOS, com operações privilegiadas auditadas via polkit.

| Componente | O que é |
|---|---|
| **App desktop** | Tauri 2 + React/TypeScript — Dashboard, Discos, Boot/UEFI, Windows 11, Doctor |
| **CLI `yua`** | Mesma engine, com efeitos (spinner, tabelas, badges) e `--json` |
| **Daemon `yua-osd`** | IPC NDJSON em Unix socket, SO_PEERCRED, **polkit** para qualquer ação privilegiada |

---

## ⬇ Download & Install (fácil)

1. Abra **[Releases](https://github.com/yuaberry/yua-os-manager/releases)** neste repositório.
2. Baixe o **`yua-os-manager_1.0.1_amd64.deb`** da release mais recente.
3. Instale com dois cliques (instalador de pacotes do Mint/Ubuntu) ou no terminal:

```bash
sudo apt install ./yua-os-manager_1.0.1_amd64.deb
```

O `.deb` instala **tudo de uma vez**: app (menu "Sistema" → YUA OS MANAGER), CLI `yua`, daemon `yua-osd`, policy polkit e systemd com *socket activation* (o daemon sobe sozinho sob demanda).

**Validação pós-install:**

```bash
yua doctor        # 11 verificações reais do seu ambiente
yua daemon status # deve responder: modo system (via socket)
```

> Requisitos: Linux x86_64 com GTK3/WebKitGTK 4.1 (Mint 21+/Ubuntu 22.04+). Para compilar do código, veja [`docs/DEVELOPMENT.md`](docs/DEVELOPMENT.md).

---

## O que ele faz

- **Discos** — inventário real via `lsblk`/`udev`/`blkid`, serial, modelo, SMART honesto (avisa se `smartctl` não existe em vez de inventar dado), detecção do disco do sistema com guarda irreversível (`YUA-DISK-010`).
- **Boot / UEFI** — lista e classifica entradas (mortas, internas do firmware, removíveis), snapshot `efibootmgr -v` antes de qualquer mutação, **BootNext one-shot** (nunca reescreve BootOrder), reboot direto na BIOS via `OsIndications` (testado em firmware que suporta `BOOT_TO_FIRMWARE_UI`).
- **Windows 11** — checklist honesto com 9 sondagens reais (TPM/CPU/Secure Boot/ESP/mídia/ISO/Ventoy), gerador de `autounattend.xml` (com bypass LabConfig para hardware antigo, chaves genéricas Pro/Home), copiador de ISO para pendrive Ventoy com progresso, e o fluxo final: `yua winstall --apply` → armadilha de confirmação (`APAGAR`) → reboot para o instalador.
- **Doctor** — 11 verificações de ambiente (deps, polkit agent, permissões ESP, smartctl…).
- **Energia** — reboot/desligamento reais via systemd-logind, sempre com `--confirm` explícito.

## Segurança (o projeto inteiro foi desenhado em torno disso)

- **Fail-closed**: sem daemon, nada privilegiado roda. Daemon em modo **dev** recusa métodos privilegiados (`YUA-AUTH-002`).
- **Toda ação de efeito exige `confirm: true`** no protocolo — não existe mutação acidental.
- **Polkit por ação** (`com.yua.osd.lowrisk`), checando `pid+starttime` do cliente (não aceita pid reciclado).
- **Destrutivo** (wipe/format/deploy real): **sempre recusado** — só existirá atrás de plano+rollback validados em QEMU.
- **Auditoria JSONL** de cada decisão (permitida ou negada) em `/var/lib/yua-os-manager/audit.jsonl`.
- Erros codificados (`YUA-{DISK,BOOT,UEFI,AUTH,DEP,STATE}-NNN`) com causa técnica + recomendação.

Detalhes: [`docs/SECURITY.md`](docs/SECURITY.md) · Arquitetura: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)

## Desenvolvimento

```bash
bash scripts/bootstrap-linux.sh   # deps + compila TUDO (workspace, testes, app)
bash scripts/package-deb.sh      # gera o .deb completo
cargo test --workspace           # 50 testes
```

## Licença

MIT © Yua Devs
