#!/usr/bin/env bash
# Déploie now3pool-hub sur bs1 via Docker (réseau host, VPN wg0 10.24.42.1).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BS1="${BS1:-bs1}"
REMOTE_DIR="/opt/now3pool"
TOKEN="${NOW3POOL_TOKEN:-$(openssl rand -hex 16)}"

echo "==> Build release (hub)"
source "${HOME}/.cargo/env" 2>/dev/null || true
(cd "$ROOT" && cargo build --release -p now3pool-hub)

echo "==> Prépare bundle"
TMP="$(mktemp -d)"
trap 'rm -rf "$TMP"' EXIT
mkdir -p "$TMP/deploy"
cp "$ROOT/target/release/now3pool-hub" "$TMP/deploy/now3pool-hub"
cp "$ROOT/deploy/Dockerfile.hub" "$TMP/deploy/Dockerfile"
cp "$ROOT/deploy/docker-compose.yml" "$TMP/deploy/docker-compose.yml"
printf 'NOW3POOL_TOKEN=%s\n' "$TOKEN" > "$TMP/deploy/now3pool.env"

echo "==> Sync vers $BS1:$REMOTE_DIR"
ssh "$BS1" "mkdir -p $REMOTE_DIR"
scp -r "$TMP/deploy/"* "$BS1:$REMOTE_DIR/"
scp "$ROOT/deploy/systemd/now3pool-hub.service" "$BS1:/etc/systemd/system/now3pool-hub.service"

echo "==> Installe hub systemd sur bs1 (hors Docker — signaux tokio OK)"
ssh "$BS1" bash -s <<REMOTE
set -euo pipefail
install -m 755 $REMOTE_DIR/now3pool-hub $REMOTE_DIR/now3pool-hub
docker rm -f now3pool-hub 2>/dev/null || true
systemctl daemon-reload
systemctl enable --now now3pool-hub.service
sleep 1
systemctl status now3pool-hub.service --no-pager | head -12
REMOTE

echo "==> Token hub (à mettre dans les agents) : $TOKEN"
echo "==> Hub URL agents : ws://10.24.42.1:9470/ws"
ssh "$BS1" "docker ps --filter name=now3pool-hub --format '{{.Names}} {{.Status}}'; ss -tlnp | grep 9470 || true"
