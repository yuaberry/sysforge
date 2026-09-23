#!/usr/bin/env bash
# SYSFORGE — instala o daemon sysforge-osd em modo SISTEMA (systemd + polkit).
#
# O que este script faz (UMA senha de sudo, no seu terminal):
#   1. encerra qualquer sysforge-osd solto (spawnado via pkexec em dev/testes)
#   2. copia os binários target/release/{sysforge-osd,sysforge} para /usr/local/bin
#   3. copia as units systemd (socket + service)
#   4. instala a policy polkit com.sysforge.osd.* (SEM ela, pkcheck não resolve
#      as ações SYSFORGE e retorna 127 — lição de campo)
#   5. habilita sysforge-osd.socket: o daemon sobe AUTOMÁTICO quando algo conecta
#
# Pré-requisito: cargo build --release executado antes.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
REL="$ROOT/target/release"

[ -x "$REL/sysforge-osd" ] || { echo "✖ $REL/sysforge-osd não encontrado. Rode: cargo build --release"; exit 1; }

echo "→ encerrando instância solta de sysforge-osd (se houver)…"
sudo pkill -x sysforge-osd 2>/dev/null || true
sleep 1
sudo rm -f /run/sysforge-osd.sock

echo "→ copiando binários para /usr/local/bin"
sudo install -m 0755 "$REL/sysforge-osd" /usr/local/bin/sysforge-osd
sudo install -m 0755 "$REL/sysforge" /usr/local/bin/sysforge

echo "→ instalando units systemd"
sudo install -m 0644 "$ROOT/deploy/systemd/sysforge-osd.socket" /etc/systemd/system/sysforge-osd.socket
sudo install -m 0644 "$ROOT/deploy/systemd/sysforge-osd.service" /etc/systemd/system/sysforge-osd.service

echo "→ instalando policy polkit (com.sysforge.osd.*)"
sudo install -m 0644 "$ROOT/deploy/systemd/com.sysforge.osd.policy" /usr/share/polkit-1/actions/com.sysforge.osd.policy

echo "→ habilitando sysforge-osd.socket (daemon sobe sob demanda)"
sudo systemctl daemon-reload
sudo systemctl enable --now sysforge-osd.socket

echo
echo "✔ Instalação completa. Socket:"
systemctl status sysforge-osd.socket --no-pager | head -5
echo
echo "Valide TUDO com:"
echo "  sysforge daemon status        # deve responder modo system (via socket)"
echo "  sysforge boot next --clear    # primeira ação privilegiada: o polkit pede sua senha (uma vez; lembra por ~5min)"
echo "  sysforge install             # checklist do Windows 11"
