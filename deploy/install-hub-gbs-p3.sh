#!/usr/bin/env bash
# Déploie poolsync-hub sur gbs-p3 (VPN wg-gbs 10.87.78.22).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOST="${HOST:-gbs-p3}"
REMOTE_DIR="/opt/poolsync"
source "$ROOT/deploy/load-env.sh"
TOKEN="$POOLSYNC_TOKEN"
SECURITY_DIR="${POOLSYNC_SECURITY_DIR:-}"
TLS_CERT="${POOLSYNC_TLS_CERT:-${SECURITY_DIR:+$SECURITY_DIR/hub.crt}}"
TLS_KEY="${POOLSYNC_TLS_KEY:-${SECURITY_DIR:+$SECURITY_DIR/hub.key}}"
TLS_CA="${POOLSYNC_TLS_CA:-${SECURITY_DIR:+$SECURITY_DIR/ca.crt}}"
NODE_TOKENS_FILE="${POOLSYNC_NODE_TOKENS_FILE:-${SECURITY_DIR:+$SECURITY_DIR/node-tokens.json}}"
REQUIRE_E2E_DEFAULT="${SECURITY_DIR:+true}"
REQUIRE_E2E="${POOLSYNC_REQUIRE_E2E:-${REQUIRE_E2E_DEFAULT:-false}}"

if [[ -n "$TLS_CERT$TLS_KEY$TLS_CA" ]] && [[ -z "$TLS_CERT" || -z "$TLS_KEY" || -z "$TLS_CA" ]]; then
  echo "POOLSYNC_TLS_CERT, POOLSYNC_TLS_KEY et POOLSYNC_TLS_CA doivent être fournis ensemble" >&2
  exit 1
fi
for security_file in "$TLS_CERT" "$TLS_KEY" "$TLS_CA" "$NODE_TOKENS_FILE"; do
  [[ -z "$security_file" || -s "$security_file" ]] || {
    echo "fichier de sécurité absent ou vide: $security_file" >&2
    exit 1
  }
done

if [[ "${POOLSYNC_SKIP_BUILD:-false}" != "true" ]]; then
  echo "==> Build dashboard web"
  if ! command -v npm >/dev/null 2>&1; then
    echo "npm requis pour construire le tableau de bord" >&2
    exit 1
  fi
  (cd "$ROOT/web" && npm ci --silent && npm run build)

  echo "==> Build release (hub)"
  source "${HOME}/.cargo/env" 2>/dev/null || true
  (cd "$ROOT" && cargo build --release -p poolsync-hub)
else
  echo "==> Réutilise les artefacts hub + dashboard déjà construits"
fi
if [[ ! -s "$ROOT/web/dist/index.html" ]]; then
  echo "build Web incomplet : web/dist/index.html absent" >&2
  exit 1
fi
if [[ ! -x "$ROOT/target/release/poolsync-hub" ]]; then
  echo "binaire hub absent : $ROOT/target/release/poolsync-hub" >&2
  exit 1
fi

DEPLOY_ID="$(date +%Y%m%d%H%M%S)-$$"
REMOTE_STAGE="$REMOTE_DIR/.deploy-$DEPLOY_ID"

echo "==> Prépare le déploiement sur $HOST"
ssh "$HOST" "mkdir -p '$REMOTE_STAGE/web' '$REMOTE_STAGE/security' /var/lib/poolsync"
scp "$ROOT/target/release/poolsync-hub" "$HOST:$REMOTE_STAGE/poolsync-hub"
scp "$ROOT/deploy/systemd/poolsync-hub.service" "$HOST:$REMOTE_STAGE/poolsync-hub.service"
tar -C "$ROOT/web/dist" -cf - . | ssh "$HOST" "tar -C '$REMOTE_STAGE/web' -xf -"
HUB_EXTRA_ARGS=""
if [[ -n "$TLS_CERT" ]]; then
  scp "$TLS_CERT" "$HOST:$REMOTE_STAGE/security/hub.crt"
  scp "$TLS_KEY" "$HOST:$REMOTE_STAGE/security/hub.key"
  scp "$TLS_CA" "$HOST:$REMOTE_STAGE/security/ca.crt"
  HUB_EXTRA_ARGS="--tls-cert $REMOTE_DIR/security/hub.crt --tls-key $REMOTE_DIR/security/hub.key"
fi
if [[ -n "$NODE_TOKENS_FILE" ]]; then
  scp "$NODE_TOKENS_FILE" "$HOST:$REMOTE_STAGE/security/node-tokens.json"
  HUB_EXTRA_ARGS="${HUB_EXTRA_ARGS:+$HUB_EXTRA_ARGS }--node-tokens-file $REMOTE_DIR/security/node-tokens.json"
fi
if [[ "$REQUIRE_E2E" == "true" ]]; then
  HUB_EXTRA_ARGS="${HUB_EXTRA_ARGS:+$HUB_EXTRA_ARGS }--require-e2e"
fi
{
  printf 'POOLSYNC_TOKEN=%s\n' "$TOKEN"
  printf 'POOLSYNC_HUB_EXTRA_ARGS="%s"\n' "$HUB_EXTRA_ARGS"
} | ssh "$HOST" "umask 077; cat > '$REMOTE_STAGE/poolsync.env'"

echo "==> Active hub systemd sur $HOST"
ssh "$HOST" bash -s -- "$REMOTE_DIR" "$REMOTE_STAGE" <<'REMOTE'
set -euo pipefail
remote_dir="$1"
stage="$2"

test -x "$stage/poolsync-hub" || chmod 755 "$stage/poolsync-hub"
test -s "$stage/web/index.html"
test -s "$stage/poolsync.env"

systemctl stop poolsync-hub.service 2>/dev/null || true
install -m 755 "$stage/poolsync-hub" "$remote_dir/poolsync-hub"
install -m 600 "$stage/poolsync.env" "$remote_dir/poolsync.env"
install -m 644 "$stage/poolsync-hub.service" /etc/systemd/system/poolsync-hub.service
install -d -m 700 "$remote_dir/security"
if [[ -s "$stage/security/hub.crt" ]]; then
  install -m 644 "$stage/security/hub.crt" "$remote_dir/security/hub.crt"
  install -m 600 "$stage/security/hub.key" "$remote_dir/security/hub.key"
  install -m 644 "$stage/security/ca.crt" "$remote_dir/security/ca.crt"
fi
if [[ -s "$stage/security/node-tokens.json" ]]; then
  install -m 600 "$stage/security/node-tokens.json" "$remote_dir/security/node-tokens.json"
fi

rm -rf "$remote_dir/web.previous"
if [[ -d "$remote_dir/web" ]]; then
  mv "$remote_dir/web" "$remote_dir/web.previous"
fi
mv "$stage/web" "$remote_dir/web"

systemctl daemon-reload
systemctl enable --now poolsync-hub.service
sleep 1
set -a
source "$remote_dir/poolsync.env"
set +a
if [[ -s "$remote_dir/security/hub.crt" ]] && [[ "$POOLSYNC_HUB_EXTRA_ARGS" == *"--tls-cert"* ]]; then
  health_base="https://localhost:9470"
  curl_tls=(--cacert "$remote_dir/security/ca.crt")
else
  health_base="http://127.0.0.1:9470"
  curl_tls=()
fi
curl -fsS "${curl_tls[@]}" "$health_base/health" >/dev/null
curl -fsS "${curl_tls[@]}" -H "Authorization: Bearer ${POOLSYNC_TOKEN}" \
  "$health_base/api/status" >/dev/null
printf 'service=%s main_pid=%s\n' \
  "$(systemctl is-active poolsync-hub.service)" \
  "$(systemctl show -p MainPID --value poolsync-hub.service)"
rm -rf "$remote_dir/web.previous" "$stage"
REMOTE

echo "==> Dashboard déployé et API authentifiée"
if [[ -n "$TLS_CERT" ]]; then
  DEFAULT_HUB_AGENT_URL="wss://10.87.78.22:9470/ws"
else
  DEFAULT_HUB_AGENT_URL="ws://10.87.78.22:9470/ws"
fi
echo "==> Hub URL agents : ${HUB_AGENT_URL:-$DEFAULT_HUB_AGENT_URL}"
ssh "$HOST" "ss -tlnp | grep 9470 || true"
