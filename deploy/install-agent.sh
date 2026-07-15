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
ssh "root@$HOST" 'DEBIAN_FRONTEND=noninteractive apt-get install -y -qq xclip xdotool libnotify-bin python3-gi gir1.2-gtk-3.0 >/dev/null'

echo "==> Binaire + config pour $USER_NAME@$HOST"
ssh "root@$HOST" "install -d -o $USER_NAME -g $USER_NAME /home/$USER_NAME/.local/bin /home/$USER_NAME/.config/poolsync /home/$USER_NAME/.local/share/poolsync /home/$USER_NAME/.local/share/applications /home/$USER_NAME/.config/systemd/user /home/$USER_NAME/.config/autostart"
ssh "root@$HOST" "su - $USER_NAME -c 'export XDG_RUNTIME_DIR=/run/user/$(id -u $USER_NAME); systemctl --user stop poolsync-agent.service 2>/dev/null || true'"
scp "$ROOT/target/release/poolsync-agent" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-agent.new"
scp "$ROOT/deploy/poolsync-agent-launch.sh" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-agent-launch.sh"
scp "$ROOT/deploy/poolsync-logs.sh" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-logs"
scp "$ROOT/deploy/poolsync-watchdog.sh" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-watchdog.sh"
scp "$ROOT/deploy/write-image-clipboard.py" "root@$HOST:/home/$USER_NAME/.local/bin/write-image-clipboard.py"
scp "$ROOT/poolsync-agent/icons/poolsync-tray.png" "root@$HOST:/home/$USER_NAME/.local/share/poolsync/poolsync-tray.png"
scp "$ROOT/deploy/com.xavdp.poolsync.desktop" "root@$HOST:/home/$USER_NAME/.local/share/applications/com.xavdp.poolsync.desktop"
ssh "root@$HOST" "mv /home/$USER_NAME/.local/bin/poolsync-agent.new /home/$USER_NAME/.local/bin/poolsync-agent && chown $USER_NAME:$USER_NAME /home/$USER_NAME/.local/bin/poolsync-agent && chmod 755 /home/$USER_NAME/.local/bin/poolsync-agent"

TMP_CFG="$(mktemp)"
trap 'rm -f "$TMP_CFG"' EXIT
sed "s/POOLSYNC_TOKEN_PLACEHOLDER/$TOKEN/" "$CONFIG_SRC" > "$TMP_CFG"
scp "$TMP_CFG" "root@$HOST:/home/$USER_NAME/.config/poolsync/agent.toml"
scp "$ROOT/deploy/systemd/poolsync-agent.service" "root@$HOST:/home/$USER_NAME/.config/systemd/user/poolsync-agent.service"
scp "$ROOT/deploy/systemd/poolsync-watchdog.service" "root@$HOST:/home/$USER_NAME/.config/systemd/user/poolsync-watchdog.service"
scp "$ROOT/deploy/systemd/poolsync-watchdog.timer" "root@$HOST:/home/$USER_NAME/.config/systemd/user/poolsync-watchdog.timer"
scp "$ROOT/deploy/autostart/poolsync-agent.desktop" "root@$HOST:/home/$USER_NAME/.config/autostart/poolsync-agent.desktop"
ssh "root@$HOST" "chown -R $USER_NAME:$USER_NAME /home/$USER_NAME/.config/poolsync /home/$USER_NAME/.local/share/poolsync /home/$USER_NAME/.local/share/applications/com.xavdp.poolsync.desktop /home/$USER_NAME/.config/systemd/user/poolsync-agent.service /home/$USER_NAME/.config/systemd/user/poolsync-watchdog.service /home/$USER_NAME/.config/systemd/user/poolsync-watchdog.timer /home/$USER_NAME/.config/autostart/poolsync-agent.desktop /home/$USER_NAME/.local/bin/poolsync-agent-launch.sh /home/$USER_NAME/.local/bin/poolsync-logs /home/$USER_NAME/.local/bin/poolsync-watchdog.sh /home/$USER_NAME/.local/bin/write-image-clipboard.py && chmod 755 /home/$USER_NAME/.local/bin/poolsync-agent-launch.sh /home/$USER_NAME/.local/bin/poolsync-logs /home/$USER_NAME/.local/bin/poolsync-watchdog.sh /home/$USER_NAME/.local/bin/write-image-clipboard.py && chmod 644 /home/$USER_NAME/.local/share/poolsync/poolsync-tray.png /home/$USER_NAME/.local/share/applications/com.xavdp.poolsync.desktop /home/$USER_NAME/.config/autostart/poolsync-agent.desktop"

echo "==> Plugin Indicator XFCE (désactivé — casse le panneau si doublon)"
# scp "$ROOT/deploy/setup-xfce-indicator.sh" "root@$HOST:/tmp/setup-xfce-indicator.sh"
# ssh "root@$HOST" "chmod +x /tmp/setup-xfce-indicator.sh && /tmp/setup-xfce-indicator.sh $USER_NAME || true"

echo "==> Active service user (sans toucher Barrier)"
ssh "root@$HOST" "loginctl enable-linger $USER_NAME 2>/dev/null || true"
scp "$ROOT/deploy/poolsync-enable-user.sh" "root@$HOST:/tmp/poolsync-enable-user.sh"
ssh "root@$HOST" "chmod 755 /tmp/poolsync-enable-user.sh && chown $USER_NAME:$USER_NAME /tmp/poolsync-enable-user.sh && runuser -u $USER_NAME -- /tmp/poolsync-enable-user.sh $USER_NAME"
