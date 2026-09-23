# SYSFORGE

> Troque de sistema operacional sem medo — inclusive em computador antigo.

O **Sysforge** nasceu para quem quer mudar de SO e não sabe por onde começar: um assistente desktop (Linux) que **diagnostica, prepara a mídia, guarda as respostas da instalação e reinicia a máquina direto no instalador** — com autorização auditada a cada passo.

| Componente | O que é |
|---|---|
| **App desktop** | Tauri 2 + React — Dashboard, Discos, Boot/UEFI, **Instalar Sistema**, Doctor |
| **CLI `sysforge`** | A mesma engine, com `--json` para scripts |
| **Daemon `sysforge-osd`** | IPC Unix socket, SO_PEERCRED, **polkit** para toda ação privilegiada |

## ⬇ Download

**Site oficial (recomendado):** <https://yuaberry.github.io/sysforge/> — detecta seu sistema e baixa o pacote certo.

Ou direto do GitHub:

- **Linux (.deb — Ubuntu 20.04+/Mint 20+):** [`sysforge_amd64.deb`](https://github.com/yuaberry/sysforge/releases/latest/download/sysforge_amd64.deb)
- Todas as releases: [github.com/yuaberry/sysforge/releases](https://github.com/yuaberry/sysforge/releases)

Instalação (um pacote instala **tudo** — app, CLI, daemon, policy polkit, systemd):

```bash
sudo apt install ./sysforge_amd64.deb
```

Depois de instalar, valide seu ambiente:

```bash
sysforge doctor         # diagnóstico honesto do seu ambiente
sysforge daemon status  # deve responder: modo system (via socket)
```

## Para quem é

- **Quem nunca instalou um SO** — o checklist mostra o que falta (ISO, pendrive, BIOS) e o fluxo guiado cuida do resto.
- **Computador antigo** — o Windows 11 exige TPM 2.0 e CPU recente; o Sysforge detecta isso e inclui o **bypass de hardware no autounattend** (LabConfig), deixando você decidir com informação honesta.
- **Quem já sabe, mas quer segurança** — snapshot UEFI antes de qualquer mutação, BootNext one-shot (nunca reescreve o BootOrder), auditoria de cada ação privilegiada.

## O que ele faz hoje

- **Instalar Windows 11** (fluxo completo): checklist real com 9 sondagens → geração de `autounattend.xml` (respostas da instalação + bypass LabConfig) → cópia da ISO para pendrive Ventoy com progresso → **BootNext one-shot** → reboot direto no instalador. Um clique, ou `sysforge install --apply`.
- **Ubuntu/Mint e outros**: a engenharia de mídia/checklist já é agnóstica de SO; o fluxo guiado por distro está no roadmap (`sysforge install --target ubuntu` → avisa honestamente o estado).
- **Discos**: inventário real (`lsblk`/`udev`/`blkid`), serial, SMART honesto, guarda irreversível do disco do sistema (`SF-DISK-010`).
- **Boot/UEFI**: classifica entradas (mortas, internas, removíveis), snapshot antes de mutações, reboot direto na BIOS via `OsIndications`.
- **Doctor**: 11 verificações de ambiente, incluindo agente de diálogo polkit e headers de build.

## Segurança

- **Fail-closed**: sem daemon, nada privilegiado roda; modo dev recusa métodos privilegiados.
- **Destrutivo sempre recusado** (wipe/format) — só existirá validado em QEMU com plano+rollback.
- Toda mutação exige `confirm:true` + polkit por ação (`pid+starttime`) + snapshot prévio + auditoria JSONL.
- Códigos de erro `SF-XXX-NNN` com causa técnica + recomendação em cada um.

Detalhes: [`docs/SECURITY.md`](docs/SECURITY.md) · Arquitetura: [`docs/ARCHITECTURE.md`](docs/ARCHITECTURE.md)

## Desenvolvimento

```bash
bash scripts/bootstrap-linux.sh   # deps + compila TUDO (workspace, testes, app)
bash scripts/package-deb.sh      # gera o sysforge_amd64.deb
cargo test --workspace           # 50 testes
```

Site: [`docs/index.html`](docs/index.html) — GitHub Pages (HTML/CSS/JS puro, sem dependências) (HTML/CSS/JS puro, sem dependências).

## Licença

MIT © Yua Devs
