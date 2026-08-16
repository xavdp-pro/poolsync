#!/usr/bin/env bash
# Lance poolsync-agent (une seule instance) avec l'environnement XFCE de CET utilisateur.
# Ne s'attache jamais au display d'un autre compte (ex. zaza2).
set -euo pipefail
UID_NUM="$(id -u)"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$UID_NUM}"

env_of() {
  local pid="$1" key="$2"
  tr "\0" "\n" < "/proc/$pid/environ" 2>/dev/null \
    | grep "^${key}=" | head -1 | cut -d= -f2- || true
}

norm_display() {
  local d="${1:-}"
  d="${d%.0}"
  printf '%s' "$d"
}

CFG_DISPLAY=""
if [[ -f "${HOME}/.config/poolsync/agent.toml" ]]; then
  CFG_DISPLAY="$(grep -E '^display\s*=' "${HOME}/.config/poolsync/agent.toml" 2>/dev/null \
    | head -1 | sed -E 's/.*=\s*"([^"]+)".*/\1/' || true)"
  CFG_DISPLAY="$(norm_display "$CFG_DISPLAY")"
fi

# Sessions XFCE de cet uid uniquement, plus récente en dernier (starttime /proc).
mapfile -t SESSION_ROWS < <(
  for pid in $(pgrep -u "$UID_NUM" -x xfce4-session 2>/dev/null || true); do
    start="$(awk '{print $22}' "/proc/$pid/stat" 2>/dev/null || echo 0)"
    disp="$(norm_display "$(env_of "$pid" DISPLAY)")"
    [[ -n "$disp" ]] && printf '%s %s %s\n' "$start" "$pid" "$disp"
  done | sort -n
)

SESSION_PID=""
SESSION_DISPLAY=""
if [[ -n "$CFG_DISPLAY" ]]; then
  for row in "${SESSION_ROWS[@]:-}"; do
    [[ -z "$row" ]] && continue
    pid="${row#* }"; pid="${pid%% *}"
    disp="${row##* }"
    if [[ "$disp" == "$CFG_DISPLAY" || "$disp" == "${CFG_DISPLAY}.0" ]]; then
      SESSION_PID="$pid"
      SESSION_DISPLAY="$disp"
    fi
  done
fi
if [[ -z "$SESSION_PID" && ${#SESSION_ROWS[@]} -gt 0 ]]; then
  row="${SESSION_ROWS[-1]}"
  SESSION_PID="${row#* }"; SESSION_PID="${SESSION_PID%% *}"
  SESSION_DISPLAY="${row##* }"
fi

if [[ -z "$SESSION_PID" ]]; then
  echo "poolsync-agent-launch: aucune session XFCE pour uid $UID_NUM" >&2
  exit 1
fi

export DISPLAY="$SESSION_DISPLAY"
dbus="$(env_of "$SESSION_PID" DBUS_SESSION_BUS_ADDRESS)"
if [[ -n "$dbus" ]]; then
  export DBUS_SESSION_BUS_ADDRESS="$dbus"
else
  export DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/bus"
fi
xa="$(env_of "$SESSION_PID" XAUTHORITY)"
if [[ -n "$xa" && -f "$xa" ]]; then
  export XAUTHORITY="$xa"
else
  export XAUTHORITY="${HOME}/.Xauthority"
fi
export XDG_CURRENT_DESKTOP="${XDG_CURRENT_DESKTOP:-XFCE}"
export GDK_BACKEND=x11

# Notifications : xfce4-notifyd souvent absent après reboot → notify-send timeout.
if ! pgrep -u "$UID_NUM" -x xfce4-notifyd >/dev/null 2>&1; then
  for notifyd in \
    /usr/lib/x86_64-linux-gnu/xfce4/notifyd/xfce4-notifyd \
    /usr/lib/xfce4/notifyd/xfce4-notifyd
  do
    if [[ -x "$notifyd" ]]; then
      "$notifyd" >/dev/null 2>&1 &
      break
    fi
  done
fi

if pgrep -u "$UID_NUM" -x poolsync-agent >/dev/null 2>&1; then
  exit 0
fi
exec "$HOME/.local/bin/poolsync-agent" "$@"
