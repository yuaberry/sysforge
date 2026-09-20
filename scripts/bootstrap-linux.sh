#!/usr/bin/env bash
# YUA OS MANAGER — bootstrap do ambiente Linux (Mint 22.3 / Ubuntu Noble).
#
# Uso:
#   bash scripts/bootstrap-linux.sh --check   # só reporta (sem sudo)
#   bash scripts/bootstrap-linux.sh            # instala tudo (pede sudo)
#
# Idempotente: pode rodar quantas vezes quiser.
set -euo pipefail

# ---------------------------------------------------------------- deps ----
# Headers para COMPILAR o app desktop (Tauri 2 no Ubuntu 24.04+).
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

# Runtime RECOMENDADO: saúde de discos, imagens Windows, testes em VM.
RUNTIME_PKGS=(
  smartmontools   # smartctl — saúde REAL de discos (YUA-DEP-005 sem isto)
  nvme-cli        # diagnóstico NVMe
  xorriso         # manipulação de ISOs
  wimtools        # aplicar install.wim/ESD do Windows
  mtools          # escrever na ESP sem montar
  qemu-system-x86  # VMs para testes destrutivos SEGUROS
  qemu-utils      # qemu-img
  ovmf            # firmware UEFI para as VMs
  memtest86+      # teste de memória (boot direto)
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
    exit 0
  fi
  echo "Pacotes ausentes (${#missing[@]}):"
  printf '  %s\n' "${missing[@]}"
  echo
  echo "Para instalar tudo:  bash scripts/bootstrap-linux.sh"
  echo "Para só buildar o workspace core/daemon/CLI (sem app desktop):"
  echo "  cargo build --workspace && cargo test --workspace"
  exit 1
}

do_install() {
  local to_install=()
  for p in "${BUILD_PKGS[@]}" "${RUNTIME_PKGS[@]}"; do
    installed "$p" || to_install+=("$p")
  done
  if [ "${#to_install[@]}" -eq 0 ]; then
    echo "✔ nada a fazer — todos os pacotes já estão instalados."
    exit 0
  fi
  echo "→ instalando ${#to_install[@]} pacote(s) via apt (sudo)…"
  sudo apt-get update
  sudo apt-get install -y "${to_install[@]}"
  echo "✔ pronto. Rode 'yua doctor' para confirmar o ambiente."
}

case "$mode" in
  --check) check ;;
  install) do_install ;;
  *) echo "uso: $0 [--check|install]"; exit 2 ;;
esac
