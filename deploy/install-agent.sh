#!/usr/bin/env bash
# Installe poolsync-agent pour l'utilisateur zaza sur un portable.
# Usage: POOLSYNC_TOKEN=xxx ./install-agent.sh inspiron inspiron
#        POOLSYNC_TOKEN=xxx ./install-agent.sh acer acer
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOST="${1:?host SSH (ex: inspiron)}"
NODE="${2:?nom noeud (ex: inspiron)}"
USER_NAME="${AGENT_USER:-zaza}"
TOKEN="${POOLSYNC_TOKEN:?POOLSYNC_TOKEN requis}"

CONFIG_SRC="$ROOT/deploy/config/agent.${NODE}.toml"
if [[ ! -f "$CONFIG_SRC" ]]; then
  echo "Config introuvable: $CONFIG_SRC" >&2
  exit 1
fi

echo "==> Build release (agent)"
source "${HOME}/.cargo/env" 2>/dev/null || true
(cd "$ROOT" && cargo build --release -p poolsync-agent)

echo "==> Dépendances X11 sur $HOST"
ssh "root@$HOST" 'apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq xclip xdotool >/dev/null'

echo "==> Binaire + config pour $USER_NAME@$HOST"
ssh "root@$HOST" "install -d -o $USER_NAME -g $USER_NAME /home/$USER_NAME/.local/bin /home/$USER_NAME/.config/poolsync /home/$USER_NAME/.config/systemd/user"
scp "$ROOT/target/release/poolsync-agent" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-agent"
ssh "root@$HOST" "chown $USER_NAME:$USER_NAME /home/$USER_NAME/.local/bin/poolsync-agent && chmod 755 /home/$USER_NAME/.local/bin/poolsync-agent"

TMP_CFG="$(mktemp)"
trap 'rm -f "$TMP_CFG"' EXIT
sed "s/POOLSYNC_TOKEN_PLACEHOLDER/$TOKEN/" "$CONFIG_SRC" > "$TMP_CFG"
scp "$TMP_CFG" "root@$HOST:/home/$USER_NAME/.config/poolsync/agent.toml"
scp "$ROOT/deploy/systemd/poolsync-agent.service" "root@$HOST:/home/$USER_NAME/.config/systemd/user/poolsync-agent.service"
ssh "root@$HOST" "chown -R $USER_NAME:$USER_NAME /home/$USER_NAME/.config/poolsync /home/$USER_NAME/.config/systemd/user/poolsync-agent.service"

echo "==> Active service user (sans toucher Barrier)"
ssh "root@$HOST" "su - $USER_NAME -c 'export XDG_RUNTIME_DIR=/run/user/$(id -u $USER_NAME); systemctl --user disable --now now3pool-agent.service 2>/dev/null || true; systemctl --user daemon-reload; systemctl --user enable --now poolsync-agent.service; systemctl --user status poolsync-agent.service --no-pager | head -15'"
