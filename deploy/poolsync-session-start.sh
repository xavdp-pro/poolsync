#!/usr/bin/env bash
# Restart PoolSync after the graphical XFCE session is ready (systray needs X11).
set -euo pipefail
UID_NUM="$(id -u)"
export XDG_RUNTIME_DIR="/run/user/$UID_NUM"
export DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/bus"

wait_for_x() {
  local i
  for i in $(seq 1 45); do
    local sess
    sess="$(pgrep -u "$UID_NUM" -x xfce4-session 2>/dev/null | head -1 || true)"
    if [[ -n "$sess" ]]; then
      local disp auth
      disp="$(tr '\0' '\n' < "/proc/$sess/environ" 2>/dev/null | grep '^DISPLAY=' | cut -d= -f2- || true)"
      auth="$(tr '\0' '\n' < "/proc/$sess/environ" 2>/dev/null | grep '^XAUTHORITY=' | cut -d= -f2- || true)"
      export DISPLAY="${disp:-:0}"
      export XAUTHORITY="${auth:-$HOME/.Xauthority}"
      if xdpyinfo -display "$DISPLAY" >/dev/null 2>&1; then
        return 0
      fi
    fi
    sleep 2
  done
  return 1
}

if ! wait_for_x; then
  echo "poolsync-session-start: XFCE/X11 not ready" >&2
  exit 1
fi

systemctl --user restart poolsync-agent.service
systemctl --user start poolsync-watchdog.timer
