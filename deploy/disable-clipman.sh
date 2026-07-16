#!/usr/bin/env bash
# Désactive Clipman XFCE (conflit avec PoolSync) — garde le paquet installé.
set -euo pipefail

USER_NAME="${1:-zaza}"
HOME_DIR="/home/$USER_NAME"
AUTOSTART="$HOME_DIR/.config/autostart"

mkdir -p "$AUTOSTART"

disable_desktop() {
  local name="$1"
  local src="/etc/xdg/autostart/${name}"
  local dst="$AUTOSTART/${name}"
  if [[ -f "$src" || -f "$dst" ]]; then
    if [[ -f "$src" && ! -f "$dst" ]]; then
      cp "$src" "$dst"
    fi
    if [[ -f "$dst" ]]; then
      if grep -q '^Hidden=' "$dst" 2>/dev/null; then
        sed -i 's/^Hidden=.*/Hidden=true/' "$dst"
      else
        echo "Hidden=true" >> "$dst"
      fi
      chown "$USER_NAME:$USER_NAME" "$dst"
      echo "disabled: $dst"
    fi
  fi
}

disable_desktop xfce4-clipman-plugin-autostart.desktop
disable_desktop xfce4-clipman.desktop

# Autostart standalone (copié depuis asus)
if [[ -f "$AUTOSTART/xfce4-clipman.desktop" ]]; then
  sed -i 's/^Hidden=.*/Hidden=true/' "$AUTOSTART/xfce4-clipman.desktop" 2>/dev/null || echo "Hidden=true" >> "$AUTOSTART/xfce4-clipman.desktop"
  chown "$USER_NAME:$USER_NAME" "$AUTOSTART/xfce4-clipman.desktop"
  echo "disabled: xfce4-clipman.desktop"
fi

pkill -u "$USER_NAME" -x xfce4-clipman 2>/dev/null || true
echo "Clipman désactivé pour $USER_NAME (PoolSync gère l'historique)"
