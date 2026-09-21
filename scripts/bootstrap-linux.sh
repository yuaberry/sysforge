#!/usr/bin/env bash
# YUA OS MANAGER — bootstrap do ambiente Linux (Mint 22.3 / Ubuntu Noble).
#
# Uso:
#   bash scripts/bootstrap-linux.sh --check   # só reporta (sem sudo)
#   bash scripts/bootstrap-linux.sh            # instala TUDO e COMPILA TUDO
#   bash scripts/bootstrap-linux.sh --no-build # só instala pacotes
#
# Idempotente: pode rodar quantas vezes quiser.
# Após instalar, a fase de build faz (na ordem):
#   1. cargo build --workspace --release   (yua + yua-osd)
#   2. cargo test --workspace              (validação real)
#   3. frontend React (npm install + build)
#   4. app desktop (cargo build em apps/desktop/src-tauri)
#   5. symlinks yua/yua-osd em ~/.local/bin
set -euo pipefail
cd "$(dirname "$0")/.."
ROOT="$PWD"

# ---------------------------------------------------------------- deps ----
# Headers para COMPILAR o app desktop (Tauri 2 no Ubuntu 24.04+). CRÍTICOS.
BUILD_PKGS=(
  libwebkit2gtk-4.1-dev
  libgtk-3-dev
  libsoup-3.0-dev
  librsvg2-dev
  libayatana-appindicator3-dev
  libssl-dev
  libxdo-dev
  file
  pkg-config
  build-essential
  curl
  wget
)

# Runtime RECOMENDADO (falha aqui = aviso, não aborta).
RUNTIME_PKGS=(
  smartmontools   # smartctl — saúde REAL de discos (YUA-DEP-005 sem isto)
  nvme-cli        # diagnóstico NVMe
  xorriso         # manipulação de ISOs
  wimtools        # aplicar install.wim/ESD do Windows
  mtools          # escrever na ESP sem montar
  qemu-system-x86 # VMs para testes destrutivos SEGUROS (Fase 9)
  qemu-utils      # qemu-img
  ovmf            # firmware UEFI para as VMs
  memtest86+      # teste de memória
  testdisk        # recuperação de partições
)

mode="${1:-install}"
installed() { dpkg -s "$1" >/dev/null 2>&1; }

check() {
  local missing=()
  for p in "${BUILD_PKGS[@]}" "${RUNTIME_PKGS[@]}"; do
    installed "$p" || missing+=("$p")
  done
  if [ "${#missing[@]}" -eq 0 ]; then
    echo "✔ ambiente completo — nada a instalar."
  else
    echo "Pacotes ausentes (${#missing[@]}):"
    printf '  %s\n' "${missing[@]}"
    echo
    echo "Para instalar + compilar tudo:  bash scripts/bootstrap-linux.sh"
  fi
  # estado dos builds
  [ -x "$ROOT/target/release/yua" ] && echo "✔ CLI release pronto" || echo "· CLI ainda não compilado (release)"
  [ -x "$ROOT/target/release/yua-desktop" ] && echo "✔ app desktop pronto" || echo "· app desktop ainda não compilado"
  [ "${#missing[@]}" -eq 0 ]
}

do_install() {
  local to_install=()
  for p in "${BUILD_PKGS[@]}"; do
    installed "$p" || to_install+=("$p")
  done
  if [ "${#to_install[@]}" -gt 0 ]; then
    echo "→ instalando ${#to_install[@]} pacote(s) CRÍTICO(S) de build via apt (sudo)…"
    sudo apt-get update
    sudo apt-get install -y --no-install-recommends "${to_install[@]}"
  else
    echo "✔ pacotes de build já presentes"
  fi

  local rt_missing=()
  for p in "${RUNTIME_PKGS[@]}"; do
    installed "$p" || rt_missing+=("$p")
  done
  if [ "${#rt_missing[@]}" -gt 0 ]; then
    echo "→ instalando ${#rt_missing[@]} pacote(s) recomendado(s) de runtime (falha aqui não aborta)…"
    sudo apt-get install -y --no-install-recommends "${rt_missing[@]}" \
      || echo "⚠ alguns pacotes de runtime falharam — o doctor detalha o que falta"
  fi
}

do_build() {
  echo
  echo "════ FASE BUILD (sem sudo — só compila) ════"
  echo "→ 1/5 workspace (yua + yua-osd) release…"
  cargo build --workspace --release

  echo "→ 2/5 testes do workspace (validação real, ~10s)…"
  cargo test --workspace --release --quiet || { echo "✖ testes falharam"; exit 1; }

  echo "→ 3/5 frontend React…"
  ( cd apps/desktop && npm install --no-fund --no-audit && npm run build )

  echo "→ 4/5 app desktop (Tauri — primeira compilação demora)…"
  ( cd apps/desktop/src-tauri && cargo build --release )

  echo "→ 5/5 symlinks em ~/.local/bin…"
  mkdir -p "$HOME/.local/bin"
  ln -sf "$ROOT/target/release/yua" "$HOME/.local/bin/yua"
  ln -sf "$ROOT/target/release/yua-osd" "$HOME/.local/bin/yua-osd"
  echo "✔ yua e yua-osd no PATH (~/.local/bin)"

  echo
  echo "════ PRONTO ════"
  echo "  CLI:      yua doctor"
  echo "  Windows:  yua winstall"
  echo "  App:      ~/yua-os-manager/target/release/yua-desktop   (ou npx tauri dev em apps/desktop)"
}

case "$mode" in
  --check) check ;;
  --no-build) do_install ;;
  install)
    do_install
    do_build
    ;;
  *) echo "uso: $0 [--check|--no-build|install]"; exit 2 ;;
esac
