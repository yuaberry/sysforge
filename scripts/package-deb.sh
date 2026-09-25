#!/usr/bin/env bash
# SYSFORGE — empacota TUDO em um único .deb instalável:
#   app desktop + CLI sysforge + daemon sysforge-osd + policy polkit
#   + systemd (socket activation) + entrada de menu + ícones
#
# Uso:  bash scripts/package-deb.sh [versão]     (padrão: 1.0.0)
# Saída: target/release/sysforge_<versão>_amd64.deb
#
# Pré-requisitos (binários já compilados):
#   cargo build --release                              (workspace: sysforge, sysforge-osd)
#   cargo build --release  em apps/desktop/src-tauri   (sysforge-desktop)
set -euo pipefail

cd "$(dirname "$0")/.."
ROOT="$PWD"
VER="${1:-1.4.0}"
REL="$ROOT/target/release"
APP_REL="$ROOT/apps/desktop/src-tauri/target/release"
OUT="$REL/sysforge_amd64.deb"

for f in "$REL/sysforge" "$REL/sysforge-osd" "$APP_REL/sysforge-desktop" \
         "$ROOT/deploy/systemd/com.sysforge.osd.policy" \
         "$ROOT/deploy/desktop/sysforge.desktop"; do
  [ -f "$f" ] || { echo "✖ faltando: $f — compile antes (cargo build --release)"; exit 1; }
done

# GUARD anti-regressão: binário DEV (modo dev do Tauri) NUNCA pode ser
# empacotado — ele procura o Vite em localhost:5173 (tela branca na
# máquina do usuário). Jeito certo: cd apps/desktop && npx tauri build --no-bundle
if ! strings "$APP_REL/sysforge-desktop" 2>/dev/null | grep 'assets/index-' > /dev/null; then
  echo "✖ FATAL: frontend NÃO embutido no sysforge-desktop (binário DEV → tela branca)"
  echo "  Compile com:  cd apps/desktop && npx tauri build --no-bundle"
  exit 1
fi

STAGE="$(mktemp -d)"
D="$STAGE/sysforge_${VER}_amd64"
trap 'rm -rf "$STAGE"' EXIT

mkdir -p "$D/DEBIAN" \
  "$D/usr/local/bin" \
  "$D/usr/lib/systemd/system" \
  "$D/usr/share/polkit-1/actions" \
  "$D/usr/share/applications" \
  "$D/usr/share/icons/hicolor/32x32/apps" \
  "$D/usr/share/icons/hicolor/128x128/apps" \
  "$D/usr/share/icons/hicolor/256x256/apps" \
  "$D/usr/share/doc/sysforge"

install -m 0755 "$REL/sysforge" "$REL/sysforge-osd" "$APP_REL/sysforge-desktop" "$D/usr/local/bin/"
install -m 0644 "$ROOT/deploy/systemd/sysforge-osd.socket" "$ROOT/deploy/systemd/sysforge-osd.service" "$D/usr/lib/systemd/system/"
install -m 0644 "$ROOT/deploy/systemd/com.sysforge.osd.policy" "$D/usr/share/polkit-1/actions/"
install -m 0644 "$ROOT/deploy/desktop/sysforge.desktop" "$D/usr/share/applications/"

ICONS="$ROOT/apps/desktop/src-tauri/icons"
install -m 0644 "$ICONS/32x32.png"      "$D/usr/share/icons/hicolor/32x32/apps/sysforge.png"
install -m 0644 "$ICONS/128x128.png"    "$D/usr/share/icons/hicolor/128x128/apps/sysforge.png"
install -m 0644 "$ICONS/128x128@2x.png" "$D/usr/share/icons/hicolor/256x256/apps/sysforge.png"

install -m 0644 "$ROOT/README.md" "$D/usr/share/doc/sysforge/"
printf 'SYSFORGE %s — Yua Devs\n' "$VER" > "$D/usr/share/doc/sysforge/copyright"

cat > "$D/DEBIAN/control" <<EOF
Package: sysforge
Version: $VER
Section: admin
Priority: optional
Architecture: amd64
Depends: libwebkit2gtk-4.1-0, libgtk-3-0t64, libayatana-appindicator3-1, libxdo3, mate-polkit
Conflicts: yua-os-manager
Replaces: yua-os-manager
Maintainer: Yua Devs <yuaberry@users.noreply.github.com>
Homepage: https://github.com/yuaberry/sysforge
Description: SYSFORGE — gerenciador de sistema e instalação do Windows 11
 App desktop (Tauri 2 + React), CLI sysforge e daemon sysforge-osd com IPC
 privilegiado via polkit, socket activation systemd e auditoria de ações.
EOF

cat > "$D/DEBIAN/postinst" <<'EOF'
#!/bin/bash
set -e
systemctl daemon-reload || true
# encerra daemon solto (spawnado via pkexec em dev) que segure o socket
if [ -S /run/sysforge-osd.sock ] && ! systemctl is-active --quiet sysforge-osd.service; then
  pkill -x sysforge-osd 2>/dev/null || true
  rm -f /run/sysforge-osd.sock
fi
systemctl enable --now sysforge-osd.socket >/dev/null 2>&1 || true
install -d -m 0755 /var/lib/sysforge
echo "sysforge: sysforge-osd.socket ativo — o daemon sobe sob demanda."
echo "Valide: sysforge daemon status"
EOF

cat > "$D/DEBIAN/prerm" <<'EOF'
#!/bin/bash
systemctl stop sysforge-osd.service sysforge-osd.socket >/dev/null 2>&1 || true
systemctl disable sysforge-osd.socket >/dev/null 2>&1 || true
pkill -x sysforge-osd 2>/dev/null || true
rm -f /run/sysforge-osd.sock
exit 0
EOF
chmod 0755 "$D/DEBIAN/postinst" "$D/DEBIAN/prerm"

dpkg-deb --build --root-owner-group "$D" "$OUT" >/dev/null
echo "✔ $OUT ($(du -h "$OUT" | cut -f1))"
