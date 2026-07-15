#!/usr/bin/env bash
# Active poolsync-agent + watchdog via systemd user (session graphique requise).
set -euo pipefail
USER_NAME="${1:-zaza}"
UIDN="$(id -u "$USER_NAME")"
export XDG_RUNTIME_DIR="/run/user/$UIDN"
export DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/bus"

if [[ ! -S "$DBUS_SESSION_BUS_ADDRESS" ]] && [[ ! -S "$XDG_RUNTIME_DIR/bus" ]]; then
  echo "Pas de bus user (session non connectée?) : $XDG_RUNTIME_DIR/bus" >&2
  exit 1
fi

systemctl --user daemon-reload
systemctl --user enable poolsync-agent.service poolsync-watchdog.timer
systemctl --user start poolsync-agent.service poolsync-watchdog.timer
systemctl --user is-active poolsync-agent.service poolsync-watchdog.timer
pgrep -a poolsync-agent || true
