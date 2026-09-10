#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"

if rg -n '\?token=|[?&]token=' \
  "$ROOT/poolsync-agent" "$ROOT/poolsync-hub" "$ROOT/web/src" "$ROOT/deploy" \
  -g '*.rs' -g '*.js' -g '*.jsx' -g '*.sh' -g '!**/tests/**'; then
  echo "security-v2-test: secret encore transporté dans une URL" >&2
  exit 1
fi

grep -F 'XChaCha20Poly1305' "$ROOT/poolsync-core/src/lib.rs" >/dev/null
grep -F 'EncryptedClipboard' "$ROOT/poolsync-hub/src/main.rs" >/dev/null
grep -F 'x-poolsync-node' "$ROOT/poolsync-hub/src/main.rs" >/dev/null
grep -F 'previous_tokens' "$ROOT/poolsync-hub/src/main.rs" >/dev/null
grep -F 'revoked' "$ROOT/poolsync-hub/src/main.rs" >/dev/null
grep -F 'wl-paste' "$ROOT/poolsync-agent/src/clipboard.rs" >/dev/null
grep -F 'wl-copy' "$ROOT/poolsync-agent/src/clipboard.rs" >/dev/null
grep -F 'ydotool' "$ROOT/poolsync-agent/src/kvm_wayland.rs" >/dev/null
grep -F 'token = "MIGRATION_DISABLED"' "$ROOT/deploy/install-agent.sh" >/dev/null
[[ "$(grep -F -c 's#peer_url = "ws://#peer_url = "wss://#' "$ROOT/deploy/install-agent.sh")" -eq 2 ]]
[[ "$(grep -F -c 's#peer_url_vpn = "ws://#peer_url_vpn = "wss://#' "$ROOT/deploy/install-agent.sh")" -eq 2 ]]
grep -F 'authentication_token()' "$ROOT/poolsync-agent/src/clipboard_history.rs" >/dev/null

TMP_DIR="$(mktemp -d)"
trap 'rm -rf "$TMP_DIR"' EXIT
bash "$ROOT/deploy/generate-security.sh" "$TMP_DIR/security" 127.0.0.1 desk-a desk-b >/dev/null
python3 - "$TMP_DIR/security/node-tokens.json" <<'PY'
import json, sys
data = json.load(open(sys.argv[1], encoding="utf-8"))
assert set(data["nodes"]) == {"desk-a", "desk-b"}
assert all(len(v["token"]) >= 40 and not v["revoked"] for v in data["nodes"].values())
PY
python3 - "$TMP_DIR/security/nodes/desk-a.toml" <<'PY'
import sys, tomllib
data = tomllib.load(open(sys.argv[1], "rb"))
assert data["node_token"]
assert data["e2e_key"]
assert data["peer_tls_cert"].endswith("/desk-a.crt")
assert data["peer_tls_key"].endswith("/desk-a.key")
assert data["peer_tokens"]["desk-b"]
PY
[[ "$(stat -c %a "$TMP_DIR/security/hub.key")" == 600 ]]
[[ "$(stat -c %a "$TMP_DIR/security/node-tokens.json")" == 600 ]]
openssl verify -CAfile "$TMP_DIR/security/ca.crt" "$TMP_DIR/security/hub.crt" >/dev/null
openssl verify -CAfile "$TMP_DIR/security/ca.crt" "$TMP_DIR/security/nodes/desk-a.crt" >/dev/null

# Les URL pair-à-pair sont souvent des IP. Le certificat du nœud cible doit
# être reproductible avec ces IP dans ses SAN, sinon WSS échoue au hostname.
mkdir -p "$TMP_DIR/config"
cat > "$TMP_DIR/config/agent.desk-a.toml" <<'EOF'
node = "desk-a"
hub_url = "wss://hub.invalid/ws"
token = "test"
mode = "full"
[screen]
width = 100
height = 100
[[neighbors]]
direction = "right"
node = "desk-b"
peer_url = "wss://192.0.2.42:9472/ws"
peer_url_vpn = "wss://peer-b.vpn.invalid:9472/ws"
EOF
POOLSYNC_CONFIG_DIR="$TMP_DIR/config" \
  bash "$ROOT/deploy/generate-security.sh" "$TMP_DIR/security-with-san" 127.0.0.1 desk-a desk-b >/dev/null
openssl verify -CAfile "$TMP_DIR/security-with-san/ca.crt" \
  -verify_ip 192.0.2.42 "$TMP_DIR/security-with-san/nodes/desk-b.crt" >/dev/null
openssl verify -CAfile "$TMP_DIR/security-with-san/ca.crt" \
  -verify_hostname peer-b.vpn.invalid "$TMP_DIR/security-with-san/nodes/desk-b.crt" >/dev/null

echo "security-v2-test: URL auth, identities, E2E, TLS PKI and Wayland backends present"
