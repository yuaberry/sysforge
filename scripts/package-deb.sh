#!/usr/bin/env bash
# YUA OS MANAGER — empacota TUDO em um único .deb instalável:
#   app desktop + CLI yua + daemon yua-osd + policy polkit
#   + systemd (socket activation) + entrada de menu + ícones
#
# Uso:  bash scripts/package-deb.sh [versão]     (padrão: 1.0.0)
# Saída: target/release/yua-os-manager_<versão>_amd64.deb
#
# Pré-requisitos (binários já compilados):
#   cargo build --release                              (workspace: yua, yua-osd)
#   cargo build --release  em apps/desktop/src-tauri   (yua-desktop)
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
VER="${1:-1.0.1}"
REL="$ROOT/target/release"
APP_REL="$ROOT/apps/desktop/src-tauri/target/release"
OUT="$REL/yua-os-manager_${VER}_amd64.deb"

for f in "$REL/yua" "$REL/yua-osd" "$APP_REL/yua-desktop" \
         "$ROOT/deploy/systemd/com.yua.osd.policy" \
         "$ROOT/deploy/desktop/yua-os-manager.desktop"; do
  [ -f "$f" ] || { echo "✖ faltando: $f — compile antes (cargo build --release)"; exit 1; }
done

STAGE="$(mktemp -d)"
D="$STAGE/yua-os-manager_${VER}_amd64"
trap 'rm -rf "$STAGE"' EXIT

mkdir -p "$D/DEBIAN" \
  "$D/usr/local/bin" \
  "$D/usr/lib/systemd/system" \
  "$D/usr/share/polkit-1/actions" \
  "$D/usr/share/applications" \
  "$D/usr/share/icons/hicolor/32x32/apps" \
  "$D/usr/share/icons/hicolor/128x128/apps" \
  "$D/usr/share/icons/hicolor/256x256/apps" \
  "$D/usr/share/doc/yua-os-manager"

install -m 0755 "$REL/yua" "$REL/yua-osd" "$APP_REL/yua-desktop" "$D/usr/local/bin/"
install -m 0644 "$ROOT/deploy/systemd/yua-osd.socket" "$ROOT/deploy/systemd/yua-osd.service" "$D/usr/lib/systemd/system/"
install -m 0644 "$ROOT/deploy/systemd/com.yua.osd.policy" "$D/usr/share/polkit-1/actions/"
install -m 0644 "$ROOT/deploy/desktop/yua-os-manager.desktop" "$D/usr/share/applications/"

ICONS="$ROOT/apps/desktop/src-tauri/icons"
install -m 0644 "$ICONS/32x32.png"      "$D/usr/share/icons/hicolor/32x32/apps/yua-os-manager.png"
install -m 0644 "$ICONS/128x128.png"    "$D/usr/share/icons/hicolor/128x128/apps/yua-os-manager.png"
install -m 0644 "$ICONS/128x128@2x.png" "$D/usr/share/icons/hicolor/256x256/apps/yua-os-manager.png"

install -m 0644 "$ROOT/README.md" "$D/usr/share/doc/yua-os-manager/"
printf 'YUA OS MANAGER %s — Yua Devs\n' "$VER" > "$D/usr/share/doc/yua-os-manager/copyright"

cat > "$D/DEBIAN/control" <<EOF
Package: yua-os-manager
Version: $VER
Section: admin
Priority: optional
Architecture: amd64
Depends: libwebkit2gtk-4.1-0, libgtk-3-0t64, libayatana-appindicator3-1, libxdo3
Maintainer: Yua Devs <yuaberry@users.noreply.github.com>
Homepage: https://github.com/yuaberry/yua-os-manager
Description: YUA OS MANAGER — gerenciador de sistema e instalação do Windows 11
 App desktop (Tauri 2 + React), CLI yua e daemon yua-osd com IPC
 privilegiado via polkit, socket activation systemd e auditoria de ações.
EOF

cat > "$D/DEBIAN/postinst" <<'EOF'
#!/bin/bash
set -e
systemctl daemon-reload || true
# encerra daemon solto (spawnado via pkexec em dev) que segure o socket
if [ -S /run/yua-osd.sock ] && ! systemctl is-active --quiet yua-osd.service; then
  pkill -x yua-osd 2>/dev/null || true
  rm -f /run/yua-osd.sock
fi
systemctl enable --now yua-osd.socket >/dev/null 2>&1 || true
install -d -m 0755 /var/lib/yua-os-manager
echo "yua-os-manager: yua-osd.socket ativo — o daemon sobe sob demanda."
echo "Valide: yua daemon status"
EOF

cat > "$D/DEBIAN/prerm" <<'EOF'
#!/bin/bash
systemctl stop yua-osd.service yua-osd.socket >/dev/null 2>&1 || true
systemctl disable yua-osd.socket >/dev/null 2>&1 || true
pkill -x yua-osd 2>/dev/null || true
rm -f /run/yua-osd.sock
exit 0
EOF
chmod 0755 "$D/DEBIAN/postinst" "$D/DEBIAN/prerm"

dpkg-deb --build --root-owner-group "$D" "$OUT" >/dev/null
echo "✔ $OUT ($(du -h "$OUT" | cut -f1))"
