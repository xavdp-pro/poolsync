#!/usr/bin/env bash
# Suite de tests CLI PoolSync — anti-régression (texte, images, historique, cache).
#
# Usage:
#   ./deploy/poolsync-test.sh              # tout (rust + intégration asus↔acer)
#   ./deploy/poolsync-test.sh --local      # rust + scripts locaux seulement
#   ./deploy/poolsync-test.sh --quick      # sans E2E réseau inter-nœuds
#   POOLSYNC_TOKEN=xxx ./deploy/poolsync-test.sh asus acer
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TOKEN="${POOLSYNC_TOKEN:-}"
HUB_HTTP="${HUB_HTTP:-http://10.24.42.1:9470}"
HUB_WS="${HUB_WS:-ws://10.24.42.1:9470/ws}"
CFG="${HOME}/.config/poolsync/agent.toml"

LOCAL_ONLY=0
QUICK=0
NODES=()

while [[ $# -gt 0 ]]; do
  case "$1" in
    --local) LOCAL_ONLY=1; shift ;;
    --quick) QUICK=1; shift ;;
    -h|--help)
      sed -n '2,8p' "$0"
      exit 0
      ;;
    *)
      NODES+=("$1")
      shift
      ;;
  esac
done

if [[ ${#NODES[@]} -eq 0 ]]; then
  NODES=(asus acer)
fi

if [[ -z "$TOKEN" && -f "$CFG" ]]; then
  TOKEN="$(grep -E '^token\s*=' "$CFG" | head -1 | sed -E 's/.*"([^"]+)".*/\1/')"
fi
TOKEN="${TOKEN:-}"

PASS=0
FAIL=0
SKIP=0
TMPDIR="${TMPDIR:-/tmp}/poolsync-test-$$"
mkdir -p "$TMPDIR"
trap 'rm -rf "$TMPDIR"' EXIT

log()  { printf '[%s] %s\n' "$(date '+%H:%M:%S')" "$*"; }
ok()   { log "OK   $*"; PASS=$((PASS + 1)); }
ko()   { log "FAIL $*"; FAIL=$((FAIL + 1)); }
skip() { log "SKIP $*"; SKIP=$((SKIP + 1)); }

xfce_env() {
  local pid
  pid="$(pgrep -u "$(id -u)" -x xfce4-session 2>/dev/null | head -1 || true)"
  if [[ -n "$pid" && -r "/proc/$pid/environ" ]]; then
    tr '\0' '\n' < "/proc/$pid/environ" | rg '^(DISPLAY|XAUTHORITY|DBUS_SESSION_BUS_ADDRESS)=' | sed 's/^/export /'
  else
    echo "export DISPLAY=${DISPLAY:-:0}"
    echo "export XAUTHORITY=${XAUTHORITY:-$HOME/.Xauthority}"
  fi
}

node_online() {
  local n="$1"
  curl -sf "${HUB_HTTP}/api/status" | python3 -c \
    "import sys,json; d=json.load(sys.stdin); sys.exit(0 if any(x.get('name')=='$n' for x in d.get('nodes',[])) else 1)" 2>/dev/null
}

node_agent_active() {
  local node="$1"
  case "$node" in
    asus)
      export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"
      systemctl --user is-active poolsync-agent 2>/dev/null
      ;;
    acer)
      ssh -o ConnectTimeout=8 acer "su - zaza -c 'export XDG_RUNTIME_DIR=/run/user/\$(id -u); systemctl --user is-active poolsync-agent 2>/dev/null'" 2>/dev/null || \
      timeout 8 ssh -o ConnectTimeout=5 "$ACER_SSH" "export XDG_RUNTIME_DIR=/run/user/\$(id -u); systemctl --user is-active poolsync-agent 2>/dev/null" 2>/dev/null || echo inactive
      ;;
    *)
      skip "agent check inconnu: $node"
      return 1
      ;;
  esac
}

node_ssh_ok() {
  local node="$1"
  case "$node" in
    asus) return 0 ;;
    acer) timeout 8 ssh -o ConnectTimeout=5 -o BatchMode=yes "$ACER_SSH" "true" 2>/dev/null ;;
    *) return 1 ;;
  esac
}

ACER_SSH="${POOLSYNC_ACER_SSH:-acer-zaza}"

node_run() {
  local node="$1"; shift
  case "$node" in
    asus) eval "$(xfce_env)"; bash -lc "$*";;
    acer)
      timeout 12 ssh -o ConnectTimeout=8 -o ServerAliveInterval=5 "$ACER_SSH" \
        "export DISPLAY=:0 XAUTHORITY=/home/zaza/.Xauthority; $*"
      ;;
    *) return 1;;
  esac
}

node_set_text() {
  local node="$1" text="$2"
  node_run "$node" "printf '%s' $(printf '%q' "$text") | timeout 5 xclip -selection clipboard"
}

node_set_text_or_skip() {
  local node="$1" text="$2" label="$3"
  if node_set_text "$node" "$text" 2>/dev/null; then
    return 0
  fi
  skip "$label (écriture $node — xclip bloqué si agent actif)"
  return 1
}

node_get_text() {
  local node="$1"
  node_run "$node" "xclip -selection clipboard -o 2>/dev/null || true"
}

node_set_image() {
  local node="$1" mime="$2" file="$3"
  case "$node" in
    asus)
      eval "$(xfce_env)"
      xclip -selection clipboard -t "$mime" < "$file"
      ;;
    acer)
      scp -q -o ConnectTimeout=8 "$file" "${ACER_SSH}:/tmp/poolsync-test-img.bin"
      timeout 12 ssh -o ConnectTimeout=8 "$ACER_SSH" \
        "export DISPLAY=:0 XAUTHORITY=/home/zaza/.Xauthority; timeout 5 sh -c $(printf '%q' "xclip -selection clipboard -t '$mime' < /tmp/poolsync-test-img.bin; rm -f /tmp/poolsync-test-img.bin")"
      ;;
    *)
      return 1
      ;;
  esac
}

hub_wait_preview() {
  local src="$1" preview="$2" tries="${3:-24}"
  local i
  for i in $(seq 1 "$tries"); do
    sleep 0.5
    if curl -sf "${HUB_HTTP}/api/clipboard/history?token=${TOKEN}&limit=30" \
      | PREVIEW="$preview" SRC="$src" python3 -c "
import json, os, sys
src = os.environ['SRC']
preview = os.environ['PREVIEW']
items = json.load(sys.stdin).get('items', [])
sys.exit(0 if any(i.get('source_node') == src and i.get('preview') == preview for i in items) else 1)
" 2>/dev/null; then
      return 0
    fi
  done
  return 1
}

node_image_bytes() {
  local node="$1" mime="${2:-image/png}"
  node_run "$node" "xclip -selection clipboard -t '$mime' -o 2>/dev/null | wc -c"
}

hub_history_count() {
  curl -sf "${HUB_HTTP}/api/clipboard/history?token=${TOKEN}&limit=50" \
    | python3 -c "import sys,json; print(len(json.load(sys.stdin).get('items',[])))"
}

count_json_cache() {
  local dir="${HOME}/.cache/poolsync/clipboard"
  if [[ ! -d "$dir" ]]; then
    echo 0
    return
  fi
  find "$dir" -maxdepth 1 -name '*.json' 2>/dev/null | wc -l | tr -d '[:space:]'
}

wait_for() {
  local desc="$1" tries="$2"; shift 2
  local i out
  for i in $(seq 1 "$tries"); do
    sleep 0.5
    if out="$("$@" 2>/dev/null)" && [[ -n "$out" ]]; then
      ok "$desc (${i})"
      return 0
    fi
  done
  ko "$desc timeout"
  return 1
}

# --- 1. Tests Rust (unitaires) ---
log "=== 1. Tests Rust (cargo test) ==="
if command -v cargo >/dev/null 2>&1; then
  # shellcheck source=/dev/null
  source "${HOME}/.cargo/env" 2>/dev/null || true
  if (cd "$ROOT" && cargo test -p poolsync-core -p poolsync-agent --quiet 2>&1); then
    ok "cargo test poolsync-core + poolsync-agent"
  else
    ko "cargo test"
  fi
else
  skip "cargo absent"
fi

# Chemins images de test (écriture presse-papiers plus bas, après E2E réseau).
PNG="${HOME}/.local/share/poolsync/poolsync-tray.png"
JPEG="${TMPDIR}/test.jpg"
if [[ -f "$PNG" ]]; then
  python3 -c "from PIL import Image; Image.open('$PNG').convert('RGB').save('$JPEG', quality=85)" 2>/dev/null \
    || cp "$PNG" "$JPEG"
fi

run_local_image_scripts() {
  log "=== Scripts image locaux (asus) ==="
  eval "$(xfce_env)"
  if [[ ! -f "$PNG" ]]; then
    skip "icône test absente ($PNG)"
    return
  fi

  if xclip -selection clipboard -t image/png < "$PNG" 2>/dev/null; then
    ok "xclip write png"
  else
    ko "xclip write png"
  fi

  if sz="$(xclip -selection clipboard -t image/png -o 2>/dev/null | wc -c)" && [[ "$sz" -gt 100 ]]; then
    ok "xclip read png ($sz bytes)"
  else
    ko "xclip read png"
  fi

  if [[ -x "${HOME}/.local/bin/write-image-clipboard.py" ]]; then
    if timeout 4 python3 "${HOME}/.local/bin/write-image-clipboard.py" < "$PNG" 2>/dev/null; then
      ok "write-image-clipboard.py"
    else
      ko "write-image-clipboard.py"
    fi
  else
    skip "write-image-clipboard.py absent"
  fi

  if [[ -x "${HOME}/.local/bin/read-image-clipboard.py" ]]; then
    xclip -selection clipboard -t image/png < "$PNG" 2>/dev/null || true
    if sz="$(timeout 4 python3 "${HOME}/.local/bin/read-image-clipboard.py" 2>/dev/null | wc -c)" && [[ "$sz" -gt 100 ]]; then
      ok "read-image-clipboard.py ($sz bytes)"
    else
      ko "read-image-clipboard.py"
    fi
  else
    skip "read-image-clipboard.py absent"
  fi
}

if [[ "$LOCAL_ONLY" -eq 1 ]]; then
  run_local_image_scripts
  log "=== RÉSUMÉ: $PASS OK, $FAIL FAIL, $SKIP SKIP ==="
  [[ "$FAIL" -eq 0 ]]
fi

# --- 2. Hub + agents ---
log "=== 2. Hub et agents ==="
if [[ -z "$TOKEN" ]]; then
  ko "POOLSYNC_TOKEN / agent.toml manquant"
  log "=== RÉSUMÉ: $PASS OK, $FAIL FAIL, $SKIP SKIP ==="
  exit 1
fi

if curl -sf "${HUB_HTTP}/health" | grep -q ok; then
  ok "hub /health"
else
  ko "hub injoignable"
  log "=== RÉSUMÉ: $PASS OK, $FAIL FAIL, $SKIP SKIP ==="
  exit 1
fi

ONLINE_JSON="$(curl -sf "${HUB_HTTP}/api/status")"
for n in "${NODES[@]}"; do
  if echo "$ONLINE_JSON" | python3 -c "import sys,json; d=json.load(sys.stdin); sys.exit(0 if any(x['name']=='$n' for x in d.get('nodes',[])) else 1)"; then
    ok "hub: $n en ligne"
  else
    ko "hub: $n absent"
  fi
  st="$(node_agent_active "$n" || true)"
  if [[ "$st" == "active" ]]; then
    ok "agent $n actif"
  else
    ko "agent $n inactif ($st)"
  fi
  if [[ "$n" != "asus" ]]; then
    if node_ssh_ok "$n"; then
      ok "ssh $n rapide"
    else
      ko "ssh $n lent ou injoignable (tests écriture $n ignorés)"
    fi
  fi
done

# --- 3. E2E texte ---
if [[ "$QUICK" -eq 0 ]]; then
  log "=== 3. E2E texte ==="
  for pair in "asus:acer" "acer:asus"; do
    src="${pair%%:*}"; dst="${pair##*:}"
    if ! node_online "$src" || ! node_online "$dst"; then
      skip "texte $src->$dst (nœud hors ligne)"
      continue
    fi
    if [[ "$src" != "asus" ]] && ! node_ssh_ok "$src"; then
      skip "texte $src->$dst (ssh $src)"
      continue
    fi
    msg="POOLSYNC-TXT-${src}-${dst}-$(date +%s)-$RANDOM"
    if ! node_set_text_or_skip "$src" "$msg" "texte $src->$dst"; then
      continue
    fi
    if hub_wait_preview "$src" "$msg"; then
      ok "texte $src->$dst (hub)"
    else
      ko "texte $src->$dst (hub timeout)"
    fi
  done

  # --- 4. E2E images ---
  log "=== 4. E2E images ==="
  if [[ -f "$PNG" ]]; then
    for pair in "asus:acer" "acer:asus"; do
      src="${pair%%:*}"; dst="${pair##*:}"
      if ! node_online "$src" || ! node_online "$dst"; then
        skip "image $src->$dst"
        continue
      fi
      if [[ "$src" != "asus" ]] && ! node_ssh_ok "$src"; then
        skip "image $src->$dst (ssh $src)"
        continue
      fi
      if ! node_set_image "$src" "image/png" "$PNG" 2>/dev/null; then
        skip "image png $src->$dst (écriture $src)"
        continue
      fi
      sz=0
      for _ in $(seq 1 20); do
        sleep 0.5
        sz="$(node_image_bytes "$dst" "image/png" | tr -d ' ')"
        [[ "${sz:-0}" -gt 500 ]] && break
      done
      if [[ "${sz:-0}" -gt 500 ]]; then
        ok "image png $src->$dst ($sz bytes)"
      else
        ko "image png $src->$dst ($sz bytes)"
      fi
    done

    if [[ -f "$JPEG" ]]; then
      for pair in "asus:acer" "acer:asus"; do
        src="${pair%%:*}"; dst="${pair##*:}"
        if ! node_online "$src" || ! node_online "$dst"; then
          skip "jpeg $src->$dst"
          continue
        fi
        node_set_image "$src" "image/jpeg" "$JPEG" 2>/dev/null || { skip "jpeg $src->$dst (écriture $src)"; continue; }
        sz=0
        for _ in $(seq 1 20); do
          sleep 0.5
          sz="$(node_image_bytes "$dst" "image/jpeg" 2>/dev/null | tr -d ' ')"
          [[ "${sz:-0}" -gt 100 ]] && break
          sz="$(node_image_bytes "$dst" "image/png" 2>/dev/null | tr -d ' ')"
          [[ "${sz:-0}" -gt 100 ]] && break
        done
        if [[ "${sz:-0}" -gt 100 ]]; then
          ok "image jpeg $src->$dst ($sz bytes)"
        else
          ko "image jpeg $src->$dst ($sz bytes)"
        fi
      done
    fi
  else
    skip "E2E image (png test absent)"
  fi
else
  skip "E2E réseau (--quick)"
fi

# --- 5. Vidage historique (régression cache local) ---
log "=== 5. Vidage historique ==="
mkdir -p "${HOME}/.cache/poolsync/clipboard"
echo '{"hash":"fake","mime":"text/plain","data":"x","preview":"x","source_node":"test","at":0,"is_image":false}' \
  > "${HOME}/.cache/poolsync/clipboard/regression-test.json"
before_cache="$(count_json_cache)"

if [[ -x "${HOME}/.local/bin/poolsync-ctl" ]]; then
  "${HOME}/.local/bin/poolsync-ctl" clear-history >/dev/null 2>&1 || true
else
  curl -sf -X POST "${HUB_HTTP}/api/clipboard/clear?token=${TOKEN}" >/dev/null
  rm -rf "${HOME}/.cache/poolsync/clipboard"
fi
sleep 3

after_hub="$(hub_history_count 2>/dev/null || echo -1)"
after_cache="$(count_json_cache)"

if [[ "$after_hub" == "0" ]]; then
  ok "hub historique vide"
else
  ko "hub historique non vide ($after_hub)"
fi
if [[ "$before_cache" -gt 0 && "$after_cache" -eq 0 ]]; then
  ok "cache local vidé ($before_cache → 0)"
elif [[ "$after_cache" -eq 0 ]]; then
  ok "cache local vide"
else
  ko "cache local encore présent ($after_cache fichiers)"
fi

run_local_image_scripts

log "=== RÉSUMÉ: $PASS OK, $FAIL FAIL, $SKIP SKIP ==="
[[ "$FAIL" -eq 0 ]]
