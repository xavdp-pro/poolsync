#!/usr/bin/env bash
# Tests CLI presse-papiers PoolSync entre nœuds.
# Usage: POOLSYNC_TOKEN=xxx ./deploy/test-clipboard.sh [asus acer inspiron]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
TOKEN="${POOLSYNC_TOKEN:-4974920bd42233517cf12325a0700ad4}"
HUB_HTTP="${HUB_HTTP:-http://10.24.42.1:9470}"
HUB_WS="${HUB_WS:-ws://10.24.42.1:9470/ws}"
if [[ $# -eq 0 ]]; then
  NODES=(asus acer inspiron)
else
  NODES=("$@")
fi

PASS=0
FAIL=0
SKIP=0

log() { printf '[%s] %s\n' "$(date '+%H:%M:%S')" "$*"; }
ok()  { log "OK   $*"; PASS=$((PASS + 1)); }
ko()  { log "FAIL $*"; FAIL=$((FAIL + 1)); }
skip(){ log "SKIP $*"; SKIP=$((SKIP + 1)); }

node_ssh() {
  local node="$1"; shift
  case "$node" in
    asus)     bash -lc "$*" ;;
    acer)     ssh -p 777 zaza@10.24.42.4 "$*" ;;
    inspiron) ssh inspiron "$*" ;;
    gbs-p2)   ssh -p 777 zaza@10.24.42.18 "$*" ;;
    *)        ko "nœud inconnu: $node"; return 1 ;;
  esac
}

node_agent_active() {
  local node="$1"
  case "$node" in
    asus)
      export XDG_RUNTIME_DIR="/run/user/$(id -u)"
      systemctl --user is-active poolsync-agent 2>/dev/null
      ;;
    acer|inspiron)
      ssh "root@$node" "runuser -u zaza -- env XDG_RUNTIME_DIR=/run/user/1001 DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1001/bus systemctl --user is-active poolsync-agent 2>/dev/null" 2>/dev/null || echo inactive
      ;;
    gbs-p2)
      ssh -p 777 zaza@10.24.42.18 "export XDG_RUNTIME_DIR=/run/user/\$(id -u); systemctl --user is-active poolsync-agent 2>/dev/null" || echo inactive
      ;;
  esac
}

node_set_clipboard() {
  local node="$1" text="$2"
  case "$node" in
    asus)
      export DISPLAY="${DISPLAY:-:0}"
      printf '%s' "$text" | xclip -selection clipboard
      ;;
    acer)
      ssh -p 777 zaza@10.24.42.4 "export DISPLAY=:0; printf '%s' \"${text}\" | xclip -selection clipboard"
      ;;
    inspiron)
      ssh root@inspiron "runuser -u zaza -- env DISPLAY=:0 XAUTHORITY=/home/zaza/.Xauthority bash -c 'printf %s \"${text}\" | xclip -selection clipboard'"
      ;;
    gbs-p2)
      ssh -p 777 zaza@10.24.42.18 "export DISPLAY=:10; printf '%s' \"${text}\" | xclip -selection clipboard"
      ;;
    *)
      node_ssh "$node" "export DISPLAY=:0; printf '%s' \"${text}\" | xclip -selection clipboard"
      ;;
  esac
}

node_get_clipboard() {
  local node="$1"
  case "$node" in
    asus)
      export DISPLAY="${DISPLAY:-:0}"
      xclip -selection clipboard -o 2>/dev/null || true
      ;;
    acer)
      ssh -p 777 zaza@10.24.42.4 "export DISPLAY=:0; xclip -selection clipboard -o 2>/dev/null || true"
      ;;
    inspiron)
      ssh root@inspiron "runuser -u zaza -- env DISPLAY=:0 XAUTHORITY=/home/zaza/.Xauthority xclip -selection clipboard -o 2>/dev/null || true"
      ;;
    gbs-p2)
      ssh -p 777 zaza@10.24.42.18 "export DISPLAY=:10; xclip -selection clipboard -o 2>/dev/null || true"
      ;;
    *)
      node_ssh "$node" "export DISPLAY=:0; xclip -selection clipboard -o 2>/dev/null || true"
      ;;
  esac
}

# --- Test 1: hub reachable ---
log "=== Test 1: hub HTTP ==="
if curl -sf "${HUB_HTTP}/health" | grep -q ok; then
  ok "hub /health"
else
  ko "hub injoignable ${HUB_HTTP}"
  exit 1
fi

# --- Test 2: nodes online ---
log "=== Test 2: nœuds connectés au hub ==="
ONLINE_JSON="$(curl -sf "${HUB_HTTP}/api/status")"
for n in "${NODES[@]}"; do
  if echo "$ONLINE_JSON" | python3 -c "import sys,json; d=json.load(sys.stdin); sys.exit(0 if any(x['name']=='$n' for x in d.get('nodes',[])) else 1)"; then
    ok "hub voit $n en ligne"
  else
    ko "hub ne voit pas $n"
  fi
done

# --- Test 3: agents systemd actifs ---
log "=== Test 3: agents locaux actifs ==="
for n in "${NODES[@]}"; do
  st="$(node_agent_active "$n" || true)"
  if [[ "$st" == "active" ]]; then
    ok "agent $n actif"
  else
    ko "agent $n inactif ($st)"
  fi
done

# --- Test 4: routage hub via WebSocket (inject clipboard) ---
log "=== Test 4: routage hub WebSocket ==="
TEST_MSG="poolsync-cli-test-$(date +%s)-$RANDOM"
export TEST_MSG TOKEN HUB_WS HUB_HTTP
set +e
python3 <<'PY'
import json, os, sys, hashlib, uuid
try:
    import websocket
except ImportError:
    print("SKIP websocket-client non installé (pip install websocket-client)")
    sys.exit(2)

token = os.environ["TOKEN"]
hub = os.environ["HUB_WS"] + "?token=" + token
text = os.environ["TEST_MSG"]
h = hashlib.sha256(text.encode()).hexdigest()
msg = json.dumps({
    "type": "clipboard",
    "msg_id": str(uuid.uuid4()),
    "hash": h,
    "mime": "text/plain",
    "data": text,
})
hello = json.dumps({
    "type": "hello",
    "node": "cli-test-sender",
    "mode": "clipboard_only",
    "screen": {"width": 1920, "height": 1080},
    "neighbors": [],
    "kvm_enabled": False,
})

ws = websocket.create_connection(hub, timeout=10)
ws.send(hello)
ws.recv()  # may get topology_update
ws.send(msg)
ws.close()

import urllib.request
import time
time.sleep(0.5)
st = json.load(urllib.request.urlopen(os.environ["HUB_HTTP"] + "/api/status"))
last = (st.get("clipboard") or {}).get("last_hash")
if last == h:
    print("OK hub a enregistré le hash clipboard")
    sys.exit(0)
print(f"FAIL hash hub={last!r} attendu={h!r}")
sys.exit(1)
PY
rc=$?
set -e
if [[ $rc -eq 0 ]]; then ok "routage hub clipboard"; elif [[ $rc -eq 2 ]]; then skip "test WebSocket (module manquant)"; else ko "routage hub clipboard"; fi

# --- Test 5: E2E entre paires de nœuds ---
log "=== Test 5: E2E copier-coller entre nœuds ==="
PAIRS=("asus:acer" "acer:asus" "asus:inspiron" "inspiron:asus")
for pair in "${PAIRS[@]}"; do
  src="${pair%%:*}"
  dst="${pair##*:}"
  marker="POOLSYNC-E2E-${src}-to-${dst}-$(date +%s)-$RANDOM"

  if ! echo "$ONLINE_JSON" | python3 -c "import sys,json; d=json.load(sys.stdin); names={x['name'] for x in d['nodes']}; sys.exit(0 if '$src' in names and '$dst' in names else 1)"; then
    skip "E2E $src->$dst (nœud hors ligne)"
    continue
  fi

  log "E2E $src -> $dst : $marker"
  if ! node_set_clipboard "$src" "$marker" 2>/dev/null; then
    ko "E2E $src->$dst : impossible d'écrire clipboard sur $src"
    continue
  fi

  got=""
  for i in $(seq 1 20); do
    sleep 0.5
    got="$(node_get_clipboard "$dst" | tr -d '\0')"
    if [[ "$got" == "$marker" ]]; then
      ok "E2E $src->$dst (${i}s)"
      break
    fi
  done
  if [[ "$got" != "$marker" ]]; then
    ko "E2E $src->$dst timeout (reçu: ${got:0:60})"
  fi
done

# --- Résumé ---
log "=== RÉSUMÉ: $PASS OK, $FAIL FAIL, $SKIP SKIP ==="
[[ $FAIL -eq 0 ]]
