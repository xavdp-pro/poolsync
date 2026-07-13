#!/usr/bin/env bash
# Lance poolsync-agent (une seule instance) avec environnement XFCE.
set -euo pipefail
UID_NUM="$(id -u)"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$UID_NUM}"
export DBUS_SESSION_BUS_ADDRESS="${DBUS_SESSION_BUS_ADDRESS:-unix:path=$XDG_RUNTIME_DIR/bus}"
export DISPLAY="${DISPLAY:-:0}"
export XAUTHORITY="${XAUTHORITY:-$HOME/.Xauthority}"
export XDG_CURRENT_DESKTOP="${XDG_CURRENT_DESKTOP:-XFCE}"
export GDK_BACKEND=x11
if pgrep -x poolsync-agent >/dev/null 2>&1; then
  exit 0
fi
exec "$HOME/.local/bin/poolsync-agent" "$@"
