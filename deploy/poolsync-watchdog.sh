#!/usr/bin/env bash
# Surveille wg-bs1 + hub PoolSync — reconnecte l'agent quand le VPN revient.
# Ne démarre PAS wg-bs1 (l'utilisateur le gère manuellement).
set -euo pipefail

CFG="${HOME}/.config/poolsync/agent.toml"
LOG_TAG="poolsync-watchdog"
CACHE_DIR="${HOME}/.cache/poolsync"
WG_STATE="${CACHE_DIR}/wg-bs1-up"
HUB_STATE="${CACHE_DIR}/hub-up"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

log() { logger -t "$LOG_TAG" "$*" 2>/dev/null || echo "[$LOG_TAG] $*"; }

if [[ ! -f "$CFG" ]]; then
  exit 0
fi
mkdir -p "$CACHE_DIR"

NODE="$(grep -E '^node\s*=' "$CFG" | head -1 | sed -E 's/.*=\s*"([^"]+)".*/\1/')"
HUB_URL="$(grep -E '^hub_url\s*=' "$CFG" | head -1 | sed -E 's/.*=\s*"([^"]+)".*/\1/')"
HUB_HOST="$(echo "$HUB_URL" | sed -E 's|^wss?://([^/:]+).*|\1|')"
HUB_PORT="$(echo "$HUB_URL" | sed -E 's|^wss?://[^/:]+:([0-9]+).*|\1|')"
HUB_PORT="${HUB_PORT:-9470}"

hub_tcp_up() {
  timeout 2 bash -c "exec 3<>/dev/tcp/${HUB_HOST}/${HUB_PORT}" 2>/dev/null
}

wg_bs1_up() {
  ip link show wg-bs1 &>/dev/null 2>&1
}

node_online() {
  curl -sf --max-time 3 "http://${HUB_HOST}:${HUB_PORT}/api/status" 2>/dev/null \
    | python3 -c "import json,sys; d=json.load(sys.stdin); m=[x for x in d.get('nodes',[]) if x.get('name')=='${NODE}']; sys.exit(0 if m and m[0].get('online') else 1)" 2>/dev/null
}

read_prev() {
  local f="$1"
  if [[ -f "$f" ]]; then
    cat "$f"
  else
    echo "0"
  fi
}

wg_now=0
hub_now=0
wg_bs1_up && wg_now=1
hub_tcp_up && hub_now=1

wg_prev="$(read_prev "$WG_STATE")"
hub_prev="$(read_prev "$HUB_STATE")"

reconnect_agent() {
  log "$1"
  systemctl --user restart poolsync-agent.service 2>/dev/null || true
}

# wg-bs1 remonté manuellement (0 → 1) : reconnecter tout de suite.
if [[ "$wg_now" == "1" && "$wg_prev" == "0" && -f "$WG_STATE" ]]; then
  reconnect_agent "wg-bs1 revenu — reconnexion PoolSync"
elif [[ "$hub_now" == "1" && "$hub_prev" == "0" && -f "$HUB_STATE" ]]; then
  reconnect_agent "hub joignable — reconnexion PoolSync"
elif [[ "$hub_now" == "1" ]]; then
  if ! systemctl --user is-active --quiet poolsync-agent.service 2>/dev/null; then
    reconnect_agent "hub joignable — démarrage poolsync-agent"
  elif ! node_online; then
    reconnect_agent "nœud ${NODE} absent du hub — reconnexion"
  fi
fi

echo "$wg_now" > "$WG_STATE"
echo "$hub_now" > "$HUB_STATE"
