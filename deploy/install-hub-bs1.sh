#!/usr/bin/env bash
# Déploie poolsync-hub sur bs1 (systemd, VPN wg0 10.24.42.1).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BS1="${BS1:-bs1}"
REMOTE_DIR="/opt/poolsync"
TOKEN="${POOLSYNC_TOKEN:-$(openssl rand -hex 16)}"

echo "==> Build release (hub)"
source "${HOME}/.cargo/env" 2>/dev/null || true
(cd "$ROOT" && cargo build --release -p poolsync-hub)

echo "==> Build dashboard web"
WEB_BUILT=0
if [[ -f "$ROOT/web/package.json" ]]; then
  if (cd "$ROOT/web" && npm ci --silent 2>/dev/null && npm run build 2>/dev/null); then
    WEB_BUILT=1
  else
    echo "    build local échoué — tentative sur $BS1 (node 20)"
    ssh "$BS1" "command -v npm >/dev/null && node -v" || { echo "npm absent sur bs1" >&2; exit 1; }
    ssh "$BS1" "rm -rf /tmp/poolsync-web-build && mkdir -p /tmp/poolsync-web-build"
    rsync -az --exclude node_modules --exclude dist "$ROOT/web/" "$BS1:/tmp/poolsync-web-build/"
    ssh "$BS1" "cd /tmp/poolsync-web-build && npm ci --silent && npm run build && mkdir -p $REMOTE_DIR/web && cp -r dist/* $REMOTE_DIR/web/"
    WEB_BUILT=1
  fi
else
  echo "web/ absent — hub sans dashboard" >&2
fi

echo "==> Prépare bundle"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/deploy/web"
cp "$ROOT/target/release/poolsync-hub" "$TMP/deploy/poolsync-hub"
cp "$ROOT/deploy/Dockerfile.hub" "$TMP/deploy/Dockerfile"
cp "$ROOT/deploy/docker-compose.yml" "$TMP/deploy/docker-compose.yml"
if [[ "$WEB_BUILT" == 1 && -d "$ROOT/web/dist" ]]; then
  cp -r "$ROOT/web/dist/"* "$TMP/deploy/web/"
fi
printf 'POOLSYNC_TOKEN=%s\n' "$TOKEN" > "$TMP/deploy/poolsync.env"

echo "==> Sync vers $BS1:$REMOTE_DIR"
ssh "$BS1" "mkdir -p $REMOTE_DIR"
scp -r "$TMP/deploy/"* "$BS1:$REMOTE_DIR/"
scp "$ROOT/deploy/systemd/poolsync-hub.service" "$BS1:/etc/systemd/system/poolsync-hub.service"

echo "==> Installe hub systemd sur bs1"
ssh "$BS1" bash -s <<REMOTE
set -euo pipefail
systemctl stop poolsync-hub.service 2>/dev/null || true
chmod 755 $REMOTE_DIR/poolsync-hub
systemctl disable --now now3pool-hub.service 2>/dev/null || true
docker rm -f now3pool-hub 2>/dev/null || true
systemctl daemon-reload
systemctl enable --now poolsync-hub.service
sleep 1
systemctl status poolsync-hub.service --no-pager | head -12
REMOTE

echo "==> Token hub (à mettre dans les agents) : $TOKEN"
echo "==> Hub URL agents : ws://10.24.42.1:9470/ws"
ssh "$BS1" "ss -tlnp | grep 9470 || true"
