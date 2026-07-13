#!/usr/bin/env bash
# Lance poolsync-agent (une seule instance) avec environnement XFCE.
set -euo pipefail
UID_NUM="$(id -u)"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$UID_NUM}"

# D-Bus : session graphique xrdp/XFCE (xfce4-session) avant bus systemd user
if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" ]]; then
  session_dbus=""
  if session_pid=$(pgrep -u "$UID_NUM" -x xfce4-session 2>/dev/null | head -1); then
    session_dbus="$(tr "\0" "\n" < "/proc/$session_pid/environ" 2>/dev/null \
      | grep '^DBUS_SESSION_BUS_ADDRESS=' | cut -d= -f2- || true)"
  fi
  if [[ -n "$session_dbus" ]]; then
    export DBUS_SESSION_BUS_ADDRESS="$session_dbus"
  else
    export DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/bus"
  fi
fi

# Display : session xrdp (:10) si active, sinon :0 XFCE local
if [[ -z "${DISPLAY:-}" || "${DISPLAY}" == ":0" ]]; then
  CFG_DISPLAY=""
  if [[ -f "${HOME}/.config/poolsync/agent.toml" ]]; then
    CFG_DISPLAY="$(grep -E '^display\s*=' "${HOME}/.config/poolsync/agent.toml" 2>/dev/null | head -1 | sed -E 's/.*"([^"]+)".*/\1/' || true)"
  fi
  if [[ -n "$CFG_DISPLAY" ]]; then
    export DISPLAY="$CFG_DISPLAY"
  elif pgrep -u "$UID_NUM" -f 'Xorg :10' >/dev/null 2>&1; then
    export DISPLAY=":10"
  else
    export DISPLAY="${DISPLAY:-:0}"
  fi
fi
export XAUTHORITY="${XAUTHORITY:-$HOME/.Xauthority}"
export XDG_CURRENT_DESKTOP="${XDG_CURRENT_DESKTOP:-XFCE}"
export GDK_BACKEND=x11
if pgrep -x poolsync-agent >/dev/null 2>&1; then
  exit 0
fi
exec "$HOME/.local/bin/poolsync-agent" "$@"
