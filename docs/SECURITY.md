# Segurança — YUA OS MANAGER

## Modelo de ameaças

Uma ferramenta que particiona discos e manipula boot pode destruir dados. As ameaças consideradas:

1. **Disco errado apagado** (renumeração USB/hot-plug no meio da operação);
2. **Command injection** (nomes de arquivos/discos com caracteres hostis);
3. **Elevação silenciosa** (app pedir sudo sem transparência);
4. **UI falsa** (botões que fingem operação);
5. **Manipulação de arquivos de estado** (operação "retomada" adulterada);
6. **Vazamento de segredos** (chaves de ativação, senhas em log);
7. **Uso acidental do disco de desenvolvimento**.

## Barreiras implementadas (Fases 1–2 parcial)

| # | Barreira | Onde | Código |
|---|---|---|---|
| 1 | Identidade do disco (model+serial+size) revalidada milissegundos antes de qualquer destrutivo | `executor.rs` | `YUA-DISK-009` |
| 2 | Host-disk guard: disco do rootfs vivo NUNCA recebe destrutivo | `executor.rs` | `YUA-DISK-010` |
| 3 | Destrutivo exige `operation_id` persistido antes | `executor.rs` | `YUA-STATE-001` |
| 4 | Sem shell: `Command::new(program).args([...])` sempre | `executor.rs` | — |
| 5 | Timeout e locale determinístico (`LC_ALL=C`) em todo comando | `executor.rs` | — |
| 6 | Fail-closed do daemon: destrutivo futuro recusado em TODOS os modos | `daemon/auth.rs` | `YUA-AUTH-002` / `YUA-AUTH-004` |
| 7 | Identidade real do chamador via `SO_PEERCRED` (kernel) | `daemon/server.rs` | — |
| 8 | Métodos privilegiados (boot/energia) exigem modo SYSTEM + **pkcheck polkit** com o pid+starttime do chamador — senha pedida ao usuário na tela | `daemon/auth.rs` | `YUA-AUTH-005` |
| 9 | Confirmação explícita (`confirm:true`) obrigatória em todo método de efeito real | `daemon/handlers.rs` | `YUA-AUTH-006` |
| 10 | Snapshot completo do UEFI ANTES de qualquer mutação de boot | `core/boot/snapshot.rs` | arquivo auditável |
| 11 | Entradas internas do firmware (FvFile/VenMsg) são INTOCÁVEIS na remoção | `daemon/handlers.rs` | `YUA-BOOT-005` |
| 12 | BootNext é one-shot — BootOrder PERMANENTE nunca é escrito | `daemon/handlers.rs` | — |
| 13 | Auditoria JSONL de TODA chamada (permitida ou negada) | `daemon/server.rs` | — |
| 14 | State machine com escrita atômica + checksum SHA-256; adulteração detectada | `state.rs` | `YUA-STATE-003` |
| 15 | Autounattend NUNCA contém senhas; conta é criada no OOBE (decisão consciente do usuário) | `core/windows/unattend.rs` | testado |
| 16 | Modo full-wipe do autounattend exige confirmação DIGITADA ("APAGAR") no CLI | `cli/winstall.rs` | — |

## Autorização (Fase 2+)

Actions polkit (deploy/systemd/com.yua.osd.policy):
- `com.yua.osd.readonly` — allow_active=yes
- `com.yua.osd.lowrisk` — auth_admin_keep
- `com.yua.osd.destructive` — auth_admin (sempre pede senha, sempre por evento)

A checagem será via `pkcheck --process <pid>,<start_time> --action-id ...` usando o PID obtido por `SO_PEERCRED` — o cliente não declara quem é.

## Hardening do service (modo system)

`NoNewPrivileges`, `ProtectSystem=strict`, `ProtectHome=yes`, `PrivateTmp=yes`, `ProtectKernelTunables/Modules`, `ProtectControlGroups`, `RestrictSUIDSGID`. `ReadWritePaths` mínimo (hoje: `/var/lib/yua-os-manager`; a Fase 5 adicionará `/boot/efi com justificativa registrada aqui).

## Política de segredos

- Chaves de ativação/senhas: NUNCA em log, NUNCA em JSON de operação (campo próprio encriptado — Fase 5), NUNCA via argumento de processo.
- Tokens de API vivem em env/variáveis de sessão, jamais no repo (`.gitignore` cobre `.env*`).

## Resposta a incidentes

- Suspeita de adulteração de estado: NÃO retomar; inspecionar `operation.json` (checksum) e `audit.jsonl`; recomeçar a operação do zero.
- Erros `YUA-DISK-009/010` inesperados: coletar `yua logs`, `lsblk --json -b` e abrir issue com os códigos.
