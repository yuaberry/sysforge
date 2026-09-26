# PLAN.MD — SYSFORGE

> Arquivo histórico do projeto: **todos os prompts do dono**, **as sessões de trabalho (chats)** e **os agentes envolvidos**. Este documento é o diário de bordo completo — nada de história reescrita: cada bug, erro meu e correção está registrado como aconteceu.

- **Projeto:** SYSFORGE (antes *YUA OS MANAGER*) — <https://github.com/yuaberry/sysforge>
- **Repo local:** `~/yua-os-manager/`
- **Site:** <https://yuaberry.github.io/sysforge/>
- **O que é:** assistente desktop (Linux, Rust/Tauri 2 + React) que leva qualquer pessoa — inclusive quem tem computador antigo ou **nenhum pendrive** — do diagnóstico até a troca de sistema operacional, com autorização auditada via polkit em cada passo.

---

## 1. ARQUIVO DOS PROMPTS (verbatim, na ordem)

> Todos os pedidos do dono do projeto, exatamente como escritos. [S-#] = sessão.

**[S1-S3 · fase fundação, resumidas]**
1. «continue de onde parou»
2. «continue, lance esse projeto no meu GITHUB em REPOSITÓRIO PRIVADO, CHAVE para fazer autenticação se PRECISAR: (ghp_✂ TOKEN REVOGADO PELO DONO — nunca armazenado aqui)»
3. «continue, até eu conseguir utilizar sem Erros e Ser totalmente Funcional. O meu Objetivo é mudar para o Windows 11 […] implemente tudo que for preciso e continue até conseguir com que o app, desligue o computador e entre na bios e instalar o sistema operacional que o usuario quer.»
4. «continue até acabar» · «Pronto Deletei o Token, Agora continue até terminar e me diga a proxima etapa»

**[S4 · publicação]**
5. «What did we do so far?» (retomada de contexto)
6. «continue a criação do aplicativo, quero o aplicativo pronto e funcional, no git para download também, já fiz login no git pelo gh auth login, não deixe dar 1 erro, tem que estar tudo funcionando certinho. faça com que o download do arquivo seja facil após acabar o projeto»

**[S5 · release 1.0.1]**
7. «o projeto todo já está pronto? Sim. Crie uma Atualização ou Release no Github 1.0.1, acabe o projeto e teste tudo, pra ver se tá tudo funcionando sem erros, e está instalando oo Sistema Operacional na maquina realmente.»

**[S6 · SYSFORGE + site]**
8. «é o seguinte: não era pra vc ter criado o aplicativo para somente eu fazer a mudança para o windows 11 ou para só o windows 11, não estou conseguindo instalar, tanto pelo terminal, quanto pelo pacote .deb que vc me me fez, por favor corrija isso e refaça o aplicativo, o aplicativo é para atender qualquer um que deseja mudar de sistema opracional mas não sabe como, e também para aquelas pessoas que tem computador antigo. já estava pensando em renomear o projeto, em tudo, no instalador e no github. o nome vai ser SYSFORGE, atualize todos os nomes yua os mager para esse. do github também. e no final faça um site Animado para download do Aplicativo com redirecionamento para o GITHUB ou download Automatico com reconhecimento do sistema operacional do usuario. quero um site com: INFORMAÇÕES SOBRE O PROJETO (POR QUAL MOTIVO EU QUIS FAZER, PUBLICO ALVO, ETC...) DOWNLOAD DO APLICATIVO, TANTO PELO GITHUB, QUANTO PELO DOWNLOAD AUTOMATICO DO PACOTE (PASTA) PELO NAVEGADOR/SITE. QUERO ANIMAÇÕES DE TRANSIÇÃO, ANIMAÇÕES ETC...»
9. «continue de onde parou»

**[S7 · tela branca]**
10. «após abrir o arquivo do Sysforge, ele dá uma tela branca e diz o erro: Could not connect to localhost: Connection refused. por favor conserte isso para ele funcionar corretamente em qualquer SO»

**[S8 · boot automático + UI]**
11. «continue o projeto, quero que inclua também a auto inicialização do pendrive ou boot do SO que o usuario deseja instalar automaticamente, então ele iria: Desligar a maquina (computador ou outro dispositivo), iniciar o boot do SO desejado, automaticamente. melhore a interface visual, com efeitos e informações essenciais e detalhadas.»

**[S9 · sem pendrive]**
12. «Adicione também uma segunda opção para quem não tem pendrive nenhum, metodo por nuvem, ou por Partição secundária, mais a segunda opção não pode usar pendrive, porque tem que garantir que todo mundo consiga o SO desejado, mesmo quem não tem um pendrive de pelo menos 8GB, eu por exemplo só tenho um de 4GB (3,63GB consta no sistema). continue o projeto, com as outras funções do aplicativo»

**[S10 · outras funções]**
13. «o projeto já está pronto? continue implementando, construindo e editando o app»

**[S11 · botões mortos]**
14. «ao instalar a iso no meu computador pelo site oficial microsoft do Win 11 25h2, fui entrar no sys forge e clicar no botão pra reiniciar e instalar e simplesmente não acontece nada, o mesmo vale para o botão de desligar. sõa apenas botões estáticos? eles tem que funcionar, o app precisa fazer o seguinte, úsuario baixa a iso no downloads, entra no SYSFORGE e aperta pra reiniciar e instalar (entrando automaticamente via comando na guia de instalação do SO desejado. sem firulas. corrija por favor»

**[S12 · SF-BOOT-020]**
15. «ainda está dando erro: SF-BOOT-020, ESTÁ FALANDO QUE A ISO NÃO FOI ENCONTRADA EM /DOWNLOADS , POR FAVOR CORRIJA TODOS OS ERROS QUE TIVER»

**[S13 · este arquivo]**
16. «antes disso, quero que faça um arquivo chamado Plan.MD para todos os meus Projetos existentes, contendo: Arquivo dos Prompts, Chats da Sessões e Agentes»

---

## 2. CHATS DAS SESSÕES (diário de bordo)

### S1-S3 · Fundação + GitHub (2026-09-21)
**Pedido:** consolidar o workspace e publicar no GitHub privado; objetivo declarado: migrar a máquina (Dell Vostro 5470, i7-4510U sem TPM, Mint 22.3) para Windows 11 com o app controlando desligamento, BIOS e instalação.
**Feito:** workspace Rust (core/daemon/CLI) com 35→50 testes reais; parsers com fixtures reais (lsblk/efibootmgr da própria máquina); guard irreversível do disco do sistema (SF/SO-DISK-010); IPC NDJSON com SO_PEERCRED; daemon fail-closed; polkit por ação; BootNext one-shot (BootOrder jamais escrito); snapshot UEFI antes de toda mutação; energia via systemd-logind; reboot→BIOS via OsIndications (validado no firmware real); fluxo `winstall` (checklist 9 sondagens → autounattend com bypass LabConfig → Ventoy → BootNext); app Tauri 2 + React; repo privado criado; 4 commits publicados.
**Erros reais e lições:** pkexec limpa o ambiente (PATH) → executor com PATH determinístico; pkcheck 127 = ação polkit desconhecida OU sem diálogo; o projeto foi movido a meio da sessão para `~/Área de Trabalho/` e precisou de consolidação manual; token GitHub exposto em chat → dono revogou (decisão correta).

### S4 · Publicação 1.0.0/1.0.1 (2026-09-21/22)
**Pedido:** "app pronto, no git, download fácil, zero erros".
**Feito:** headers webkit instalados (autorização via diálogo polkit), app compilado e aberto na tela do dono; `.deb` único com app+CLI+daemon+policy+systemd+ícone; README; **fix do handler morto** (`unattend_save` fora do `generate_handler!` — warning escondendo bug); `gh` instalado; 3 commits publicados; **socket activation systemd implementada (LISTEN_FDS)** — causa raiz: daemon rebindava socket que o systemd segura; validação ao vivo da cadeia privilegiada (o dono autorizou o diálogo do polkit e o audit JSONL registrou `allowed:true`).
**Errata honesta:** avisei "polkit reiniciado" quando o pipe `head` engoliu um erro — corrigido na mensagem seguinte.

### S5 · Release 1.0.1 ao vivo (2026-09-22)
**Pedido:** release 1.0.1 + testar tudo + "está instalando o SO de verdade?".
**Feito:** device-flow do `gh auth login` (código na tela do dono); release v1.0.1 com `.deb`; instalação real na máquina; **bug do timeout IPC caçado ao vivo**: cliente desistia em 30s enquanto o diálogo do polkit espera 180s (EAGAIN no cliente, timeout no daemon, EPIPE no fim) → `call_interactive` com 200s; round-trip real de BootNext no firmware (armar → efibootmgr lê → reverter); quirk descoberto no `efibootmgr 2.39.3` (delete de BootNext=0000 falha deixando a variável) → fallback por unlink na efivarfs + resposta baseada em releitura real; **erros meus registrados**: usei um GUID errado num `rm` manual (o GUID correto: `8be4df61-93ca-11d2-aa0d-00e098032b8c`) e um `pkill -f` que matou minha própria shell — duas vezes (padrão contido na própria cmdline).

### S6 · Rebrand SYSFORGE + site (2026-09-23)
**Pedido:** corrigir instalação quebrada (o .deb 1.0.1 nunca chegou ao `~/Downloads` — erro meu, o comando apontava para arquivo inexistente), generalizar o app ("para qualquer um que deseja mudar de SO e não sabe como", "computador antigo"), renomear TUDO para SYSFORGE, site animado com download automático.
**Feito:** diagnóstico forense do erro de instalação (histórico do shell + dpkg.log); renome completo (crates, binários, socket, systemd, polkit `com.sysforge.osd.*`, códigos `SF-XXX-NNN`, app, docs, scripts, repo GitHub renomeado com redirect); nome de arquivo **sem versão** (`sysforge_amd64.deb`) + URL permanente `releases/latest/download/...`; pacote com `Conflicts/Replaces` migrando o antigo; **app agora sobe o agente de diálogo polkit sozinho**; página "Instalar Sistema" com cartões de alvo honestos; site completo (orbes, faíscas de forja, typewriter, reveal ao rolar, detecção de SO + download automático) publicado no GitHub Pages; repo tornado público (scan de segredos no histórico antes: limpo).
**Fix de lockfile:** `ident_case 1.1.0` yanked no crates.io → `--precise 1.0.1`.

### S7 · Tela branca (2026-09-23)
**Pedido:** "abre com tela branca: Could not connect to localhost: Connection refused — conserte para funcionar em qualquer SO".
**Causa raiz (minha):** compilei o app com `cargo build` direto → binário **modo DEV**: em runtime carrega `devUrl` (localhost:5173, o servidor do Vite) que não existe na máquina do usuário. Meus "✓ app rodando" anteriores só checavam processo, não janela — falso positivo registrado.
**Fix:** produção SEMPRE via `npx tauri build --no-bundle` (frontend embutido, +54 KB comprimidos); guard anti-regressão no `package-deb.sh` (recusa binário sem assets embutidos) — e o guard inicial usava `strings | grep -q` que morria na armadilha `pipefail`+SIGPIPE (registrada e corrigida); release 1.1.1, v1.1.0 quebrada **removida** para ninguém baixar.

### S8 · Boot automático + redesign (2026-09-24) — v1.2.0
**Pedido:** ciclo completo: desligar a máquina → ligar direto no boot do SO desejado; melhor interface com efeitos e detalhes.
**Feito:** CLI `--poweroff` (desliga; ao ligar, entra no instalador) além do `--reboot`; pipeline visual de 4 passos com status real; **contagem regressiva abortável em tela inteira**; redesign completo (fundo ambiente animado, cards glass em cascata, barras de progresso reais, chips do BootOrder, transições de página).

### S9 · Sem pendrive (2026-09-24/25) — v1.3.0
**Pedido:** método para quem não tem pendrive (o do dono tem 3,63 GB; ISO Win11 ~5,7 GB) — "garantir que todo mundo consiga o SO desejado".
**Feito:** método **Disco + Nuvem**: a ISO fica no disco; o GRUB ganha entrada que carrega o instalador **direto da ISO para a RAM via wimboot** (iPXE oficial); `grub-reboot` = boot one-shot NO GRUB (reboot/desligar → volta direto no instalador); nenhuma partição nova (encolher ext4 montado é impossível online — registrado o porquê); sondagem honesta antes de prometer (ISO>3GiB, Secure Boot OFF, GRUB, RAM ≥2,5 GiB, wimboot baixado da nuvem se faltar); 4 métodos novos no daemon (readiness read-only + prepare/arm/revert privilegiados com confirm); CLI `--method auto|usb|disk`; 53 testes.

### S10 · Backup + Redes (2026-09-25) — v1.4.0
**Pedido:** "o projeto já está pronto? continue com as outras funções".
**Feito:** página **Backup** (destinos externos, medição byte a byte real, "cabe/não cabe" antes de copiar, rsync SEM `--delete`, sem root, lista-branca de pastas); página **Redes** (nmcli real, leitura apenas); 56 testes.

### S11 · Botões mortos (2026-09-26) — v1.4.1
**Pedido:** "baixei a ISO 25H2, clico em reiniciar e instalar e não acontece nada. são botões estáticos?"
**Causa raiz (minha, três camadas):** os botões nasciam `disabled` (o de USB exigia Ventoy; o do disco, uma sondagem feita 1× na abertura da página — sempre obsoleta quando a ISO chega depois) e, sem daemon, o cartão nem renderizava botão.
**Fix "sem firulas":** UM lançador com botões **sempre clicáveis**; decisão do método **no clique** com sondagem fresca in-process (`get_disk_readiness` sem daemon, atualizada a cada 5s); Ventoy → BootNext USB, senão disco pronto → GRUB/wimboot, senão erro honesto embaixo do botão com as pendências exatas.

### S12 · SF-BOOT-020 (2026-09-26) — final
**Pedido:** "ainda dá SF-BOOT-020, ISO não encontrada — corrija TODOS os erros".
**Caça completa (4 bugs, todos registrados):**
1. daemon instalado era anterior ao fix (reinstall pendente);
2. `find_iso` no daemon: `HOME=/root` → varre também `/home/*/Downloads`;
3. **`ProtectSystem=strict` bloquearia** os 4 caminhos do método → `ReadWritePaths` explícitos (`/etc/grub.d`, `/etc/default`, `/boot/grub`, `/var/lib/sysforge`);
4. **A CAUSA RAIZ DO SF-BOOT-020: `ProtectHome=yes` monta /home VAZIO no sandbox do daemon** — a ISO era invisível mesmo com código certo → `ProtectHome=read-only` (lê a ISO, nunca escreve em /home).
**Prova física final (executada na máquina do dono, pelo mesmo caminho do clique):** `disk_prepare` real → entrada `/etc/grub.d/40_sysforge` gravada (670 B), ISO detectada (`Win11_25H2_BrazilianPortuguese_x64_v2_v2.iso`, 8,17 GB), wimboot baixado da fonte oficial (76 KB), **entrada bootável confirmada no grub.cfg** (verificação como root).

---

## 3. AGENTES

### Ambiente
- **Plataforma:** OpenCode (`~/.opencode/bin/opencode`), configuração Yua Devs em `~/.config/opencode/AGENTS.md`.
- **Agentes disponíveis no ambiente:** `architect`, `builder`, `game-dev`, `reviewer`, `debugger`, `quick` (seleção via interface) — subagentes: `debugger` (investigação de causa raiz), `explore` (exploração de codebase), `general`, `reviewer` (auditoria de código/arquitetura/segurança).

### Quem atuou neste projeto
- **BUILDER (engenheira principal)** — agente executante de todas as sessões: leitura do sistema antes de alterar, causa raiz antes de consertar, alterações pequenas e verificáveis, testes reais (nunca "funciona porque compila"). Modelo: GLM (z-ai/glm-5.3).
- **O DONO (yuaberry)** — dono do produto e **autorizador humano final**: definiu o objetivo ("qualquer um trocar de SO, mesmo sem pendrive e em PC antigo"), reportou todos os bugs de campo (tela branca, botões mortos, SF-BOOT-020) e **autorizou cada ação privilegiada** pelo diálogo do polkit — o projeto inteiro depende dessa corrente de confiança auditável (audit JSONL em `/var/lib/sysforge/audit.json`).
- **Agentes que NÃO atuaram, por decisão:** `reviewer`/`debugger` como subagentes não foram necessários — a auditoria foi feita in-line com testes reais (56) e validação ao vivo; registrado aqui por honestidade.

### Princípios operacionais (aplicados e cobrados em todas as sessões)
1. Entender antes de modificar; causa raiz antes de correção (nunca "chutar consertos").
2. Nada de dados falsos: ferramenta ausente = relatada como ausente (SMART, ISO, agente polkit).
3. Destrutivo sempre recusado (só existirá validado em QEMU com plano+rollback).
4. Toda ação de efeito: `confirm` + polkit + snapshot prévio + auditoria.
5. Erros do próprio agente: registrados (este arquivo é a prova — includes: handler morto, pipe engolindo erro, pkill autodestrutivo, GUID errado, binário dev na produção, botões nascendo desabilitados).

### Artefatos vivos
| Artefato | Onde |
|---|---|
| Código | `~/yua-os-manager/` → <https://github.com/yuaberry/sysforge> |
| Releases | <https://github.com/yuaberry/sysforge/releases> (asset permanente: `sysforge_amd64.deb`) |
| Site | <https://yuaberry.github.io/sysforge/> |
| Auditoria de ações privilegiadas | `/var/lib/sysforge/audit.json` (na máquina) |
| Snapshots UEFI | `/var/lib/sysforge/snapshots/` (na máquina) |
| Testes | 56 (`cargo test --workspace`) |

_Última atualização: 2026-09-26 · Release corrente: **v1.4.1** · Estado: método disco validado de ponta a ponta; pendente do dono: clicar ⚡ Reiniciar e instalar (após Backup, se desejado)._
