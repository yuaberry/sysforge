# IMPLEMENTATION STATUS — YUA OS MANAGER

Auditoria honesta do projeto. Nada aqui é inflado: cada item marcado tem
verificação executável associada.

Legenda:
- `[x]` concluído (com prova)
- `[~]` em progresso
- `[ ]` planejado (fase futura)
- `[!]` bloqueado (com motivo técnico + como resolver)

---

## Fase 1 — Fundação ✅ (Checkpoint 01 atingido)

**Prova executável:** `cargo build --workspace` verde · `cargo test --workspace`
= 35 testes passando (34 unit + 1 integração IPC) · `npm run build` do frontend
verde (tsc + vite) · CLI funcional contra o hardware real.

### [x] Workspace e consolidação
- [x] Repo consolidado em `~/yua-os-manager/` (workspace: core, daemon, cli;
      app desktop excluído até instalação dos headers — decisão consciente)
- [x] `.gitignore` cobrindo target/, node_modules/, dist/, .env*

### [x] Núcleo (`yua-core`)
- [x] Erros codificados `YUA-{DISK,BOOT,IMAGE,NET,UEFI,WIN,DEP,AUTH,STATE,...}-NNN`
      com mensagem + técnico + recomendação (testado)
- [x] Executor: `program+args` SEM shell · `LC_ALL=C` · timeout · log estruturado
- [x] Classes de risco: ReadOnly / LowRisk / Destructive
- [x] Modo DryRun (read-only roda de verdade; destrutivo apenas `WOULD_RUN`) — testado
- [x] **Host-disk guard**: destrutivo no disco do rootfs vivo recusado
      (`YUA-DISK-010`) — testado contra o /dev/sda REAL desta máquina
- [x] Destrutivo exige `operation_id` (`YUA-STATE-001`) — testado
- [x] Identidade de disco com revalidação anti-hot-plug (`YUA-DISK-009`) —
      probe real retorna serial WX61A79A2TDH do WDC — testado
- [x] State machine com grafo explícito + persistência atômica (tmp+fsync+rename)
      + checksum SHA-256 anti-adulteração (`YUA-STATE-003`) — testado
- [x] `hw/system`: os-release, meminfo, cpuinfo, uptime, bateria/AC, TPM,
      UEFI, Secure Boot (offset 4 do efivar) — parsers testados
- [x] `disk/lsblk`: parser do JSON real (util-linux 2.39, `mountpoints` array) — testado c/ fixture real
- [x] `disk/udev`: leitura de `/run/udev/data/b{maj}:{min}` sem root — testado ao vivo
- [x] `disk/blkid`, `disk/smart` (honesto: YUA-DEP-005/006 quando indisponível) — testado
- [x] `boot/efi`: parser efibootmgr completo (mesma-linha + continuação) — testado c/ fixture real
- [x] `boot/esp`: /proc/mounts + statvfs + detecção de permissão restrita — testado ao vivo
- [x] `capability`: probe de 30 ferramentas + KVM/OVMF/memtest + headers Tauri — testado
- [x] `ipc/protocol`: NDJSON Request/Response/WireError + registro destrutivo — testado
- [x] `ipc/client`: cliente unix socket com timeout e mapeamento de erro

### [x] Daemon (`yua-osd`)
- [x] Modo dev: socket 0600, mesmo-uid via SO_PEERCRED, **destrutivo recusado fail-closed (YUA-AUTH-002)**
- [x] Modo system: exige root; auditoria JSONL de TODA chamada
- [x] 6 métodos v1 read-only publicados (echo, system.info, disks.list, efi.entries, capabilities, daemon.info)
- [x] Integração IPC ponta a ponta testada (spawn real do binário + cliente + fail-closed verificado)

### [x] CLI (`yua`)
- [x] Executável de terminal SEPARADO com efeitos: banner gradiente ANSI,
      spinner braille, tabelas box-drawing, barras, badges — respeitando NO_COLOR/pipe
- [x] Comandos: `status`, `disks`, `boot`, `doctor`, `daemon`, `logs`
- [x] `--json` global para scripts; saída de erro estruturada com código
- [x] `doctor`: 10 verificações com spinner + comando apt EXATO para o que falta
- [x] Logs em `~/.local/share/yua-os-manager/logs/`

### [x] App desktop (frontend)
- [x] React 18 + Vite 5 + TS 5 + react-router-dom 6 (build verde)
- [x] Dashboard/Discos/Boot/Diagnóstico com dados reais via Tauri commands in-process
- [x] Páginas futuras = estado honesto "planejado — fase N" (zero UI falsa)
- [x] Erro de bridge honesto (YUA-BRIDGE-001) quando fora do Tauri
- [x] src-tauri renomeado: `yua-desktop` / `dev.yua.osmanager` / janela 1400×900

### [x] Deploy e docs
- [x] `deploy/systemd/`: yua-osd.socket + yua-osd.service (hardening) + com.yua.osd.policy
- [x] `scripts/bootstrap-linux.sh` (--check e instalação) e `scripts/install-daemon.sh`
- [x] docs: README, ARCHITECTURE, SECURITY, DEVELOPMENT

### [!] Bloqueios conhecidos (com resolução)
- [!] **Compilar/rodar o app desktop**: headers webkit2gtk-4.1-dev ausentes no Mint 22.3 por padrão.
      Resolver: `bash scripts/bootstrap-linux.sh` (sudo) → `cd apps/desktop/src-tauri && cargo build`.
      O workspace core/daemon/CLI compila e é 100% testável sem isto.
- [!] **Deps de runtime opcionais** (smartmontools, qemu, xorriso, wimtools, mtools, ovmf,
      memtest86+, testdisk, nvme-cli): ausentes → funcionalidades reportam indisponibilidade
      honesta (YUA-DEP-005…). Mesma resolução: bootstrap-linux.sh.
- [!] **Métodos destrutivos**: INTENCIONALMENTE não implementados (fail-closed, YUA-AUTH-004).
      Chegam na Fase 2 via polkit + Fase 9 (testes QEMU).

---

## Fase 2 (parcial) — Controle de boot/energia + fluxo Windows 11 ✅

**Prova executável:** 50 testes verdes (46 core + 3 auth pkcheck + 1 integração) ·
`yua winstall` roda checklist real (UEFI/TPM/ISO/USB/Ventoy/OsIndications/daemon) ·
`yua boot` marca entradas mortas (WIN_INSTALL `File()` vazio) · build do frontend verde.

### [x] Controle REAL de energia e BIOS
- [x] "Reiniciar direto na BIOS": `OsIndications` (bit BOOT_TO_FIRMWARE_UI) —
      mecanismo UEFI oficial, confirmado suportado pelo firmware deste Vostro (testado)
- [x] `yua boot firmware` (reboot→setup) / `--arm` (só arma o próximo boot)
- [x] `yua power reboot|off --confirm` (via systemctl, root)
- [x] Daemon system via **pkexec** — polkit pede a senha na tela do usuário
- [x] Autorização por método: **pkcheck** com pid+starttime (SO_PEERCRED) — YUA-AUTH-005

### [x] Boot controlado
- [x] `v1.boot.set_next`: BootNext ONE-SHOT com snapshot prévio; **BootOrder nunca escrito**
- [x] `yua boot next` (auto-detecta entrada USB 0012 do firmware nesta máquina)
- [x] `yua boot remove <ID>`: guarda de internas do firmware (FvFile/VenMsg intocáveis) +
      confirmação digitada + snapshot
- [x] Snapshot do estado UEFI (`efibootmgr -v`) antes de QUALQUER mutação

### [x] Fluxo Windows 11 (o objetivo declarado do usuário)
- [x] `yua winstall`: checklist honesto com 9 sondagens reais
- [x] Gerador `autounattend.xml`: bypass LabConfig (TPM/SecureBoot/RAM/CPU — hardware
      antigo instala), locale pt-BR, edição Pro/Home por chave genérica, **zero senhas no XML**
- [x] Modo full-wipe (apaga disco 0 na instalação) exige confirmação DIGITADA "APAGAR"
- [x] Cópia de ISO para pendrive **Ventoy** com progresso real de bytes
- [x] Página "Instalar Windows 11" no app: checklist + gerador + BootNext/BIOS/reboot/off
- [x] Entradas mortas detectáveis (o WIN_INSTALL quebrado desta máquina aparece marcado)

### [x] Lições de campo integradas à ferramenta
- [x] Doctor: check #11 "agente de diálogo polkit" — detecta quando o APP não
      conseguirá pedir senha na tela (sem agente + sem TTY = Request dismissed)
      e instrui a correção (autostart + reinício de sessão)
- [x] Autostart do agente MATE em `~/.config/autostart/` (instalado pela sessão)
- [x] `ensure_system_daemon` com recomendação precisa: terminal do usuário
      (pkttyagent) como caminho garantido

### [!] Bloqueios que dependem do USUÁRIO (não do código)
- [!] **Headers do app desktop + ferramentas runtime**: exigem apt (senha do usuário).
      O polkit não exibe diálogo para pedidos originados do shell do agente — o comando
      deve ser executado NO TERMINAL DO USUÁRIO: `bash scripts/bootstrap-linux.sh`.
      Depois: `cd apps/desktop/src-tauri && cargo build` (e `npx tauri dev` na raiz do app).
- [!] **ISO do Windows 11**: ausente (6+ GiB) — download manual do link oficial
      (o checklist mostra) e salvar em ~/Downloads.
- [!] **Pendrive**: nenhum conectado — inserir um com Ventoy (o checklist detecta).
- [!] **Métodos destrutivos** (wipe/format/deploy): recusados por projeto (YUA-AUTH-004)
      até a fase de deploy com plano validado + rollback + testes QEMU.

---

## Fase 2 (restante) — Planejamento de instalação `[ ]`
- [ ] Modos express/avançado/automated/recovery/custom
- [ ] Plano de instalação serializável + validação
- [ ] Métodos de escrita no daemon com pkcheck por classe de risco
- [ ] `v1.plan.create` / `v1.plan.validate`

## Fase 3 — Imagens `[ ]`
- [ ] Seleção/validação ISO (loop mount + checksum SHA-256)
- [ ] Download com retomada (netboot.xyz, imagens oficiais)
- [ ] Extração WIM/ESD (`7z`, wimlib)

## Fase 4 — Discos e particionamento `[ ]`
- [ ] Plano de partição GPT com pré-visualização
- [ ] Staging em DryRun end-to-end do particionamento
- [ ] Motor de rollback de tabela de partições (backup de cabeçalho GPT)

## Fase 5 — Deploy + Milestone 1 (UKI / BootNext) `[ ]`
- [ ] UKI: objcopy + linuxx64.efi.stub (systemd 255) — gerado, não baixado
- [ ] BootNext one-shot com snapshot prévio (BootOrder NUNCA alterado)
- [ ] Deploy Linux (rsync + fstab + initramfs) e Windows (apply unattend + WIM)

## Fase 6 — Pós-instalação `[ ]` · Fase 7 — Redes `[ ]` (nmcli) · Fase 8 — Verificação pós-boot `[ ]`

## Fase 9 — Testes destrutivos (QEMU) `[ ]`
- [ ] Suite real: particionar, formatar, instalar, bootar — só em discos virtuais
- [ ] Gate no CI: NUNCA contra /dev/sda do host (guard permanece armado)

## Fase 10 — Empacotamento `[ ]`
- [ ] .deb do par CLI+daemon; app desktop via `tauri build` (targets deb)

---

_Última atualização: Fase 1 concluída neste ambiente (workspace compilando, 35 testes verdes, CLI funcional com dados reais: WDC WD10SPZX serial WX61A79A2TDH, UEFI nativo, Secure Boot desabilitado, ESP 512 MiB restrita)._
