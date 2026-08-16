#!/usr/bin/env bash
# Déploie poolsync-hub sur gbs-p3 (VPN wg-gbs 10.87.78.22).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOST="${HOST:-gbs-p3}"
REMOTE_DIR="/opt/poolsync"
TOKEN="${POOLSYNC_TOKEN:-$(openssl rand -hex 16)}"

echo "==> Build release (hub)"
source "${HOME}/.cargo/env" 2>/dev/null || true
(cd "$ROOT" && cargo build --release -p poolsync-hub)

echo "==> Sync vers $HOST:$REMOTE_DIR"
ssh "$HOST" "mkdir -p $REMOTE_DIR/web /var/lib/poolsync"
scp "$ROOT/target/release/poolsync-hub" "$HOST:$REMOTE_DIR/poolsync-hub"
printf 'POOLSYNC_TOKEN=%s\n' "$TOKEN" | ssh "$HOST" "cat > $REMOTE_DIR/poolsync.env"
scp "$ROOT/deploy/systemd/poolsync-hub.service" "$HOST:/etc/systemd/system/poolsync-hub.service"

echo "==> Active hub systemd sur $HOST"
ssh "$HOST" bash -s <<REMOTE
set -euo pipefail
chmod 755 $REMOTE_DIR/poolsync-hub
systemctl daemon-reload
systemctl enable --now poolsync-hub.service
sleep 1
systemctl status poolsync-hub.service --no-pager | head -12
REMOTE

echo "==> Token hub : $TOKEN"
echo "==> Hub URL agents (wg-gbs) : ws://10.87.78.22:9470/ws"
ssh "$HOST" "ss -tlnp | grep 9470 || true"
