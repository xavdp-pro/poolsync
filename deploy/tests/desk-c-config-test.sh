#!/usr/bin/env bash
set -euo pipefail

TEST_DIR="$(mktemp -d)"
trap 'rm -rf "$TEST_DIR"' EXIT
ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SOURCE="$ROOT/deploy/bench/70-desk-c-xubuntu.sh"
PYTHON_SNIPPET="$TEST_DIR/upsert.py"
CONFIG="$TEST_DIR/agent.toml"

# Exécute exactement l'algorithme embarqué dans le script LXD, en remplaçant
# seulement son chemin de configuration absolu par celui du bac à sable.
awk '
  /^podman exec -i .* <<'"'"'PYTHON'"'"'$/ { inside=1; next }
  inside && /^PYTHON$/ { exit }
  inside { print }
' "$SOURCE" \
  | sed 's|path = Path("/home/zaza/.config/poolsync/agent.toml")|path = Path(os.environ["TEST_AGENT_CONFIG"])|' \
  > "$PYTHON_SNIPPET"

cat > "$CONFIG" <<'TOML'
node = "desk-b"

[[neighbors]]
direction = "left"
node = "desk-a"
peer_url = "ws://10.89.2.2:9472/ws"

[[neighbors]]
direction = "right"
node = "desk-c"
peer_url = "ws://10.89.2.99:9472/ws"
TOML

DESK_C_IP=10.89.2.4 TEST_AGENT_CONFIG="$CONFIG" python3 "$PYTHON_SNIPPET"

[[ "$(grep -c '^node = "desk-c"$' "$CONFIG")" == 1 ]]
grep -F 'peer_url = "ws://10.89.2.4:9472/ws"' "$CONFIG" >/dev/null
grep -F 'node = "desk-a"' "$CONFIG" >/dev/null
if grep -F '10.89.2.99' "$CONFIG" >/dev/null; then
  echo "desk-c-config-test: ancienne IP encore présente" >&2
  exit 1
fi

echo "desk-c-config-test: stale desk-c neighbor IP is replaced"
