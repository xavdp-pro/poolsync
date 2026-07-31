#!/usr/bin/env bash
# Contrôle PoolSync sans passer par le menu systray (quand il ne répond plus).
set -euo pipefail

UNIT="poolsync-agent.service"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

usage() {
  cat <<'EOF'
Usage: poolsync-ctl <commande>

Commandes:
  restart   Redémarre l'agent (corrige souvent le menu systray figé)
  stop      Arrête PoolSync sur cette machine
  start     Démarre PoolSync
  status    État du service + dernières lignes de log
  logs      Journal (alias: poolsync-logs)
  clear-history Vide l'historique hub + cache local
  toggle    Active/désactive PoolSync localement (équivalent Ctrl+Alt+Shift+P)

Exemples:
  poolsync-ctl restart
  poolsync-ctl stop
EOF
}

ensure_bus() {
  if [[ -z "${DBUS_SESSION_BUS_ADDRESS:-}" && -S "$XDG_RUNTIME_DIR/bus" ]]; then
    export DBUS_SESSION_BUS_ADDRESS="unix:path=$XDG_RUNTIME_DIR/bus"
  fi
}

cmd_restart() {
  ensure_bus
  echo "Redémarrage de PoolSync…"
  systemctl --user restart "$UNIT"
  sleep 1
  systemctl --user is-active --quiet "$UNIT" && echo "PoolSync actif." || {
    echo "Échec du redémarrage — voir: poolsync-ctl logs" >&2
    exit 1
  }
}

cmd_stop() {
  ensure_bus
  echo "Arrêt de PoolSync…"
  systemctl --user stop "$UNIT" 2>/dev/null || true
  if pgrep -x poolsync-agent >/dev/null 2>&1; then
    pkill -x poolsync-agent 2>/dev/null || true
    sleep 0.5
  fi
  if pgrep -x poolsync-agent >/dev/null 2>&1; then
    echo "Impossible d'arrêter poolsync-agent" >&2
    exit 1
  fi
  echo "PoolSync arrêté."
}

cmd_start() {
  ensure_bus
  echo "Démarrage de PoolSync…"
  systemctl --user start "$UNIT"
  sleep 1
  systemctl --user is-active --quiet "$UNIT" && echo "PoolSync actif." || {
    echo "Échec du démarrage — voir: poolsync-ctl logs" >&2
    exit 1
  }
}

cmd_status() {
  ensure_bus
  systemctl --user status "$UNIT" --no-pager || true
  echo "---"
  journalctl --user -u "$UNIT" -n 8 --no-pager 2>/dev/null || true
}

cmd_logs() {
  local logs_bin="${HOME}/.local/bin/poolsync-logs"
  if [[ -x "$logs_bin" ]]; then
    exec "$logs_bin" "$@"
  fi
  exec journalctl --user -u "$UNIT" -n 100 --no-pager "$@"
}

cmd_toggle() {
  # Sends SIGUSR1 if we add it later; for now notify via dbus or use notify-send hint.
  # Hotkey is handled inside the agent — restart is the reliable external toggle fallback.
  if pgrep -x poolsync-agent >/dev/null 2>&1; then
    echo "Bascule locale : utilise Ctrl+Alt+Shift+P dans la session graphique."
    echo "Si le systray ne répond pas : poolsync-ctl restart"
  else
    cmd_start
  fi
}

cmd_clear_history() {
  ensure_bus
  local cfg="${HOME}/.config/poolsync/agent.toml"
  if [[ ! -f "$cfg" ]]; then
    echo "Config introuvable: $cfg" >&2
    exit 1
  fi
  local token hub_url
  token="$(grep -E '^token\s*=' "$cfg" | head -1 | sed -E 's/.*"([^"]+)".*/\1/')"
  hub_url="$(grep -E '^hub_url\s*=' "$cfg" | head -1 | sed -E 's/.*"(ws|http)([^"]+)".*/http\2/')"
  hub_url="${hub_url%/ws}"
  echo "Vidage historique hub ($hub_url)…"
  curl -sf -X POST "${hub_url}/api/clipboard/clear?token=${token}" >/dev/null \
    || { echo "Échec vidage hub" >&2; exit 1; }
  rm -rf "${HOME}/.cache/poolsync/clipboard"
  echo "Historique vidé (hub + cache local)."
}

main() {
  local cmd="${1:-}"
  shift || true
  case "$cmd" in
    restart|r) cmd_restart ;;
    stop|quit|q) cmd_stop ;;
    start) cmd_start ;;
    status|st) cmd_status ;;
    logs|log) cmd_logs "$@" ;;
  toggle|t) cmd_toggle ;;
  clear-history|clear) cmd_clear_history ;;
  -h|--help|help|"") usage ;;
    *)
      echo "Commande inconnue: $cmd" >&2
      usage >&2
      exit 1
      ;;
  esac
}

main "$@"
