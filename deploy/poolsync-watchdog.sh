#!/usr/bin/env bash
# Surveille wg-bs1 + hub PoolSync — reconnecte l'agent quand le VPN revient.
# Ne démarre PAS wg-bs1 (l'utilisateur le gère manuellement).
set -euo pipefail

CFG="${HOME}/.config/poolsync/agent.toml"
LOG_TAG="poolsync-watchdog"
CACHE_DIR="${HOME}/.cache/poolsync"
WG_STATE="${CACHE_DIR}/wg-bs1-up"
HUB_STATE="${CACHE_DIR}/hub-up"
MISS_STATE="${CACHE_DIR}/node-misses"
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

RDP_MISS="${CACHE_DIR}/rdp-display-misses"
RDP_MISS_THRESHOLD="${POOLSYNC_RDP_MISS_THRESHOLD:-2}"
PICK_BIN="${HOME}/.local/bin/poolsync-pick-session.sh"
SKIP_HUB=0

reconnect_agent() {
  log "$1"
  echo "0" > "$MISS_STATE"
  echo "0" > "$RDP_MISS"
  SKIP_HUB=1
  systemctl --user restart poolsync-agent.service 2>/dev/null || true
}

# xrdp: rattacher l'agent à la session zaza actuellement connectée
# (xrdp-chansrv vivant), pas à un DISPLAY orphelin.

norm_display() {
  local d="${1:-}"
  d="${d%.0}"
  printf '%s' "$d"
}

agent_display() {
  local pid
  pid="$(pgrep -u "$(id -u)" -x poolsync-agent 2>/dev/null | head -1 || true)"
  [[ -n "$pid" && -r "/proc/$pid/environ" ]] || return 0
  norm_display "$(tr '\0' '\n' < "/proc/$pid/environ" 2>/dev/null | grep '^DISPLAY=' | head -1 | cut -d= -f2- || true)"
}

if [[ -x "$PICK_BIN" ]]; then
  live_out="$("$PICK_BIN" --live-rdp 2>/dev/null || true)"
  live_disp="$(norm_display "${live_out%% *}")"
  cur_disp="$(agent_display)"
  if [[ -n "$live_disp" && "$cur_disp" != "$live_disp" ]]; then
    misses=$(( $(read_prev "$RDP_MISS") + 1 ))
    if (( misses >= RDP_MISS_THRESHOLD )); then
      reconnect_agent "RDP zaza sur ${live_disp} (agent était ${cur_disp:-aucun}) — rattachement"
    else
      log "RDP ${live_disp} vs agent ${cur_disp:-aucun} (${misses}/${RDP_MISS_THRESHOLD}) — on attend"
      echo "$misses" > "$RDP_MISS"
    fi
  else
    echo "0" > "$RDP_MISS"
  fi
fi

# Un nœud peut disparaître une passe du hub sans être en panne : panel XFCE qui
# redémarre, hoquet réseau, hub lent à répondre. L'agent se reconnecte seul dans
# ces cas — le redémarrer casse la session KVM pour rien. On n'agit donc qu'après
# MISS_THRESHOLD passes ratées d'affilée.
MISS_THRESHOLD="${POOLSYNC_MISS_THRESHOLD:-3}"

if [[ "$SKIP_HUB" != "1" ]]; then
  # wg-bs1 remonté manuellement (0 → 1) : reconnecter tout de suite.
  if [[ "$wg_now" == "1" && "$wg_prev" == "0" && -f "$WG_STATE" ]]; then
    reconnect_agent "wg-bs1 revenu — reconnexion PoolSync"
  elif [[ "$hub_now" == "1" && "$hub_prev" == "0" && -f "$HUB_STATE" ]]; then
    reconnect_agent "hub joignable — reconnexion PoolSync"
  elif [[ "$hub_now" == "1" ]]; then
    if ! systemctl --user is-active --quiet poolsync-agent.service 2>/dev/null; then
      reconnect_agent "hub joignable — démarrage poolsync-agent"
    elif ! node_online; then
      misses=$(( $(read_prev "$MISS_STATE") + 1 ))
      if (( misses >= MISS_THRESHOLD )); then
        reconnect_agent "nœud ${NODE} absent du hub depuis ${misses} passes — reconnexion"
      else
        log "nœud ${NODE} absent du hub (${misses}/${MISS_THRESHOLD}) — on attend"
        echo "$misses" > "$MISS_STATE"
      fi
    else
      echo "0" > "$MISS_STATE"
    fi
  fi
fi

echo "$wg_now" > "$WG_STATE"
echo "$hub_now" > "$HUB_STATE"

# Clipman + PRIMARY sync regularly steal PNG/text from PoolSync (xrdp too).
if [[ -n "${DISPLAY:-}" ]] && command -v xfconf-query >/dev/null 2>&1; then
  xfconf-query -c xfce4-clipman -p /settings/add-primary-clipboard -n -t bool -s false 2>/dev/null || true
  xfconf-query -c xfce4-clipman -p /settings/persistent-primary-clipboard -n -t bool -s false 2>/dev/null || true
  xfconf-query -c xfce4-clipman -p /settings/add-images -n -t bool -s false 2>/dev/null || true
  xfconf-query -c xfce4-clipman -p /settings/history-ignore-primary-clipboard -n -t bool -s true 2>/dev/null || true
fi
