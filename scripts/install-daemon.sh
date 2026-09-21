#!/usr/bin/env bash
# YUA OS MANAGER — instala o daemon yua-osd em modo SISTEMA (systemd + polkit).
#
# O que este script faz (UMA senha de sudo, no seu terminal):
#   1. encerra qualquer yua-osd solto (spawnado via pkexec em dev/testes)
#   2. copia os binários target/release/{yua-osd,yua} para /usr/local/bin
#   3. copia as units systemd (socket + service)
#   4. instala a policy polkit com.yua.osd.* (SEM ela, pkcheck não resolve
#      as ações YUA e retorna 127 — lição de campo)
#   5. habilita yua-osd.socket: o daemon sobe AUTOMÁTICO quando algo conecta
#
# Pré-requisito: cargo build --release executado antes.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
REL="$ROOT/target/release"

[ -x "$REL/yua-osd" ] || { echo "✖ $REL/yua-osd não encontrado. Rode: cargo build --release"; exit 1; }

echo "→ encerrando instância solta de yua-osd (se houver)…"
sudo pkill -x yua-osd 2>/dev/null || true
sleep 1
sudo rm -f /run/yua-osd.sock

echo "→ copiando binários para /usr/local/bin"
sudo install -m 0755 "$REL/yua-osd" /usr/local/bin/yua-osd
sudo install -m 0755 "$REL/yua" /usr/local/bin/yua

echo "→ instalando units systemd"
sudo install -m 0644 "$ROOT/deploy/systemd/yua-osd.socket" /etc/systemd/system/yua-osd.socket
sudo install -m 0644 "$ROOT/deploy/systemd/yua-osd.service" /etc/systemd/system/yua-osd.service

echo "→ instalando policy polkit (com.yua.osd.*)"
sudo install -m 0644 "$ROOT/deploy/systemd/com.yua.osd.policy" /usr/share/polkit-1/actions/com.yua.osd.policy

echo "→ habilitando yua-osd.socket (daemon sobe sob demanda)"
sudo systemctl daemon-reload
sudo systemctl enable --now yua-osd.socket

echo
echo "✔ Instalação completa. Socket:"
systemctl status yua-osd.socket --no-pager | head -5
echo
echo "Valide TUDO com:"
echo "  yua daemon status        # deve responder modo system (via socket)"
echo "  yua boot next --clear    # primeira ação privilegiada: o polkit pede sua senha (uma vez; lembra por ~5min)"
echo "  yua winstall             # checklist do Windows 11"
