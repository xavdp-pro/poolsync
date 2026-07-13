#!/usr/bin/env bash
# Affiche les logs du client PoolSync (service systemd user).
set -euo pipefail

UNIT="poolsync-agent.service"
LINES="${POOLSYNC_LOG_LINES:-100}"
FOLLOW=0

usage() {
  cat <<'EOF'
Usage: poolsync-logs [-f] [-n lignes]

  -f          suivi en temps réel (comme tail -f)
  -n N        nombre de lignes récentes (défaut: 100)

Exemples:
  poolsync-logs           # 100 dernières lignes
  poolsync-logs -f        # suivi live
  poolsync-logs -n 500    # 500 dernières lignes
EOF
}

while getopts "fn:h" opt; do
  case "$opt" in
    f) FOLLOW=1 ;;
    n) LINES="$OPTARG" ;;
    h) usage; exit 0 ;;
    *) usage >&2; exit 1 ;;
  esac
done

export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

ARGS=(--user -u "$UNIT" --no-pager -n "$LINES")
[[ "$FOLLOW" -eq 1 ]] && ARGS+=(-f)

exec journalctl "${ARGS[@]}"
