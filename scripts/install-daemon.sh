#!/usr/bin/env bash
# YUA OS MANAGER — instala o daemon yua-osd em modo SISTEMA (systemd + polkit).
#
# O que este script faz (tudo com sudo explícito, nada automático):
#   1. copia os binários target/release/{yua-osd,yua} para /usr/local/bin
#   2. copia as units systemd (socket + service) para /etc/systemd/system
#   3. copia a policy polkit com.yua.osd.policy para /usr/share/polkit-1/actions
#   4. habilita e inicia o socket (activation sob demanda)
#
# Pré-requisito: cargo build --release executado antes.
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
REL="$ROOT/target/release"

[ -x "$REL/yua-osd" ] || { echo "✖ $REL/yua-osd não encontrado. Rode: cargo build --release"; exit 1; }

echo "→ copiando binários para /usr/local/bin (sudo)"
sudo install -m 0755 "$REL/yua-osd" /usr/local/bin/yua-osd
sudo install -m 0755 "$REL/yua" /usr/local/bin/yua

echo "→ instalando units systemd"
sudo install -m 0644 "$ROOT/deploy/systemd/yua-osd.socket" /etc/systemd/system/yua-osd.socket
sudo install -m 0644 "$ROOT/deploy/systemd/yua-osd.service" /etc/systemd/system/yua-osd.service

echo "→ instalando policy polkit (com.yua.osd.*)"
sudo install -m 0644 "$ROOT/deploy/systemd/com.yua.osd.policy" /usr/share/polkit-1/actions/com.yua.osd.policy

echo "→ habilitando yua-osd.socket (activation sob demanda)"
sudo systemctl daemon-reload
sudo systemctl enable --now yua-osd.socket

echo
echo "✔ Daemon instalado. Estado do socket:"
systemctl status yua-osd.socket --no-pager | head -5
echo
echo "Confira com:  yua daemon status   (deve mostrar modo system)"
