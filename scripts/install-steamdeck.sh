#!/usr/bin/env bash
set -euo pipefail

# FreePlay installer for Steam Deck (SteamOS), Bazzite, and Linux desktops.
# Installs entirely under $HOME (~/.local/bin) so immutable root filesystems stay untouched.

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
BIN_SRC="${SCRIPT_DIR}/freeplay-app"
ICON_SRC="${SCRIPT_DIR}/freeplay.png"

INSTALL_DIR="${HOME}/.local/bin"
APP_DIR="${HOME}/.local/share/applications"
ICON_DIR="${HOME}/.local/share/icons/hicolor/128x128/apps"
TARGET_BIN="${INSTALL_DIR}/freeplay"

if [[ ! -f "${BIN_SRC}" ]]; then
  echo "Error: freeplay-app binary not found next to install-steamdeck.sh" >&2
  exit 1
fi

mkdir -p "${INSTALL_DIR}" "${APP_DIR}" "${ICON_DIR}"

install -m 755 "${BIN_SRC}" "${TARGET_BIN}"
echo "Installed FreePlay binary to ${TARGET_BIN}"

if [[ -f "${ICON_SRC}" ]]; then
  install -m 644 "${ICON_SRC}" "${ICON_DIR}/freeplay.png"
fi

cat > "${APP_DIR}/freeplay.desktop" <<EOF
[Desktop Entry]
Name=FreePlay
Comment=Open-source single-player game trainer hub
Exec=${TARGET_BIN}
Icon=freeplay
Terminal=false
Type=Application
Categories=Game;Utility;
Keywords=trainer;game;memory;scanner;steamdeck;
EOF

if command -v update-desktop-database >/dev/null 2>&1; then
  update-desktop-database "${APP_DIR}" >/dev/null 2>&1 || true
fi

echo "Created desktop launcher at ${APP_DIR}/freeplay.desktop"
echo ""
echo "Granting CAP_SYS_PTRACE so FreePlay can attach to single-player games without running as root..."
echo "(On Steam Deck, set a sudo password once in Konsole with 'passwd' if you haven't already.)"

if command -v setcap >/dev/null 2>&1; then
  if sudo setcap cap_sys_ptrace=eip "${TARGET_BIN}"; then
    echo "Success! CAP_SYS_PTRACE granted to ${TARGET_BIN}."
  else
    echo "Warning: Could not grant CAP_SYS_PTRACE automatically."
    echo "You can run it manually later:"
    echo "  sudo setcap cap_sys_ptrace=eip \"${TARGET_BIN}\""
  fi
else
  echo "Warning: 'setcap' not found. You can allow memory access per session with:"
  echo "  sudo sysctl -w kernel.yama.ptrace_scope=0"
fi

echo ""
echo "Done! Launch FreePlay from your Application Menu, or right-click it in Desktop Mode -> 'Add to Steam' for Gaming Mode."
