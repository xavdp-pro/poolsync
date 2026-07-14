#!/usr/bin/env bash
# Diagnostic PoolSync KVM en CLI (sans systray).
# Usage: ./deploy/kvm-debug.sh [asus|acer|inspiron]
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NODE="${1:-asus}"
HUB="${HUB_URL:-http://10.24.42.1:9470}"
CFG="${HOME}/.config/poolsync/agent.toml"
BIN="${HOME}/.local/bin/poolsync-agent"
DURATION="${DEBUG_SECS:-12}"

export DISPLAY="${DISPLAY:-:0}"
export XAUTHORITY="${XAUTHORITY:-$HOME/.Xauthority}"
export XDG_RUNTIME_DIR="${XDG_RUNTIME_DIR:-/run/user/$(id -u)}"

echo "=== PoolSync KVM debug ($NODE) ==="
echo "DISPLAY=$DISPLAY  HUB=$HUB"

echo ""
echo "--- 1. Hub /health ---"
if curl -sf --connect-timeout 5 "${HUB}/health" >/dev/null; then
  echo "OK"
else
  echo "ÉCHEC (hub injoignable ou bloqué)"
fi

echo ""
echo "--- 2. Hub /api/status ---"
STATUS="$(curl -sf --connect-timeout 5 --max-time 8 "${HUB}/api/status" 2>/dev/null || true)"
if [[ -n "$STATUS" ]]; then
  echo "$STATUS" | python3 -m json.tool 2>/dev/null || echo "$STATUS"
else
  echo "ÉCHEC (timeout — hub peut être en deadlock, redémarrer poolsync-hub)"
fi

echo ""
echo "--- 3. X11 souris (xdotool) ---"
if command -v xdotool >/dev/null; then
  xdotool getmouselocation --shell 2>/dev/null || echo "xdotool: échec lecture souris"
  xdotool getdisplaygeometry 2>/dev/null || true
else
  echo "xdotool absent"
fi

echo ""
echo "--- 4. Agent service ---"
systemctl --user is-active poolsync-agent.service 2>/dev/null || echo "service inactif"

echo ""
echo "--- 5. Logs récents (KVM/errors) ---"
journalctl --user -u poolsync-agent -n 20 --no-pager 2>/dev/null \
  | grep -iE 'KVM|primary|error|connected|ended' || true

if [[ "${RUN_CLI_AGENT:-0}" == "1" ]]; then
  echo ""
  echo "--- 6. Agent CLI ${DURATION}s (--no-tray, RUST_LOG=debug) ---"
  systemctl --user stop poolsync-agent.service 2>/dev/null || true
  sleep 1
  export RUST_LOG="${RUST_LOG:-poolsync_agent=debug}"
  timeout "$DURATION" "$BIN" --config "$CFG" --no-tray 2>&1 || true
  systemctl --user start poolsync-agent.service 2>/dev/null || true
else
  echo ""
  echo "Pour lancer l'agent en CLI debug ${DURATION}s:"
  echo "  RUN_CLI_AGENT=1 $0 $NODE"
fi

echo ""
echo "=== fin ==="
