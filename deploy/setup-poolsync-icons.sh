#!/usr/bin/env bash
# Register poolsync-tray icon for XFCE menu / whiskermenu favorites.
set -euo pipefail

USER_NAME="${1:-zaza}"
HOME_DIR="$(getent passwd "$USER_NAME" | cut -d: -f6)"
ICON_SRC="${HOME_DIR}/.local/share/poolsync/poolsync-tray.png"

if [[ ! -f "$ICON_SRC" ]]; then
  echo "Icon missing: $ICON_SRC" >&2
  exit 1
fi

for size in 16 22 24 32 48; do
  install -d -o "$USER_NAME" -g "$USER_NAME" \
    "${HOME_DIR}/.local/share/icons/hicolor/${size}x${size}/apps"
  install -m 644 -o "$USER_NAME" -g "$USER_NAME" \
    "$ICON_SRC" "${HOME_DIR}/.local/share/icons/hicolor/${size}x${size}/apps/poolsync-tray.png"
done

if command -v gtk-update-icon-cache >/dev/null; then
  runuser -u "$USER_NAME" -- gtk-update-icon-cache -f -t \
    "${HOME_DIR}/.local/share/icons/hicolor" 2>/dev/null || true
fi

echo "Icon poolsync-tray installed for $USER_NAME"
