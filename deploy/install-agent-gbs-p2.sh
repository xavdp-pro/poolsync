#!/usr/bin/env bash
# Installe poolsync-agent pour zaza sur gbs-p2 (session xrdp :10).
# Usage: POOLSYNC_TOKEN=xxx ./install-agent-gbs-p2.sh
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
HOST="${1:-gbs-p2}"
NODE="gbs-p2"
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
ssh "root@$HOST" 'DEBIAN_FRONTEND=noninteractive apt-get install -y -qq xclip xdotool libnotify-bin libgtk-3-0 libayatana-appindicator3-1 2>/dev/null || apt-get install -y -qq xclip xdotool libnotify-bin'

echo "==> Binaire + config pour $USER_NAME@$HOST"
ssh "root@$HOST" "install -d -o $USER_NAME -g $USER_NAME /home/$USER_NAME/.local/bin /home/$USER_NAME/.config/poolsync /home/$USER_NAME/.local/share/poolsync /home/$USER_NAME/.local/share/applications /home/$USER_NAME/.config/systemd/user /home/$USER_NAME/.config/autostart"
ssh "root@$HOST" "su - $USER_NAME -c 'export XDG_RUNTIME_DIR=/run/user/\$(id -u); systemctl --user stop poolsync-agent.service 2>/dev/null || true'"

scp "$ROOT/target/release/poolsync-agent" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-agent.new"
scp "$ROOT/deploy/poolsync-agent-launch.sh" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-agent-launch.sh"
scp "$ROOT/deploy/poolsync-session-start.sh" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-session-start.sh"
scp "$ROOT/deploy/poolsync-logs.sh" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-logs"
scp "$ROOT/deploy/autostart/poolsync-agent.desktop" "root@$HOST:/home/$USER_NAME/.config/autostart/poolsync-agent.desktop"
scp "$ROOT/poolsync-agent/icons/poolsync-tray.png" "root@$HOST:/home/$USER_NAME/.local/share/poolsync/poolsync-tray.png"
scp "$ROOT/deploy/com.xavdp.poolsync.desktop" "root@$HOST:/home/$USER_NAME/.local/share/applications/com.xavdp.poolsync.desktop"
ssh "root@$HOST" "mv /home/$USER_NAME/.local/bin/poolsync-agent.new /home/$USER_NAME/.local/bin/poolsync-agent && chown $USER_NAME:$USER_NAME /home/$USER_NAME/.local/bin/poolsync-agent && chmod 755 /home/$USER_NAME/.local/bin/poolsync-agent"

TMP_CFG="$(mktemp)"
trap 'rm -f "$TMP_CFG"' EXIT
sed "s/POOLSYNC_TOKEN_PLACEHOLDER/$TOKEN/" "$CONFIG_SRC" > "$TMP_CFG"
scp "$TMP_CFG" "root@$HOST:/home/$USER_NAME/.config/poolsync/agent.toml"

# Service user avec DISPLAY :10 pour session xrdp
ssh "root@$HOST" "cat > /home/$USER_NAME/.config/systemd/user/poolsync-agent.service" <<EOF
[Unit]
Description=PoolSync agent (clipboard + KVM)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=%h/.local/bin/poolsync-agent-launch.sh --config %h/.config/poolsync/agent.toml
Restart=on-failure
RestartSec=5
Environment=DISPLAY=:11
Environment=XAUTHORITY=%h/.Xauthority
Environment=XDG_CURRENT_DESKTOP=XFCE
Environment=GDK_BACKEND=x11

[Install]
WantedBy=default.target
EOF

ssh "root@$HOST" "chown -R $USER_NAME:$USER_NAME /home/$USER_NAME/.config/poolsync /home/$USER_NAME/.local/share/poolsync /home/$USER_NAME/.local/share/applications/com.xavdp.poolsync.desktop /home/$USER_NAME/.config/systemd/user/poolsync-agent.service /home/$USER_NAME/.config/autostart/poolsync-agent.desktop /home/$USER_NAME/.local/bin/poolsync-agent-launch.sh /home/$USER_NAME/.local/bin/poolsync-session-start.sh /home/$USER_NAME/.local/bin/poolsync-logs && chmod 755 /home/$USER_NAME/.local/bin/poolsync-agent-launch.sh /home/$USER_NAME/.local/bin/poolsync-session-start.sh /home/$USER_NAME/.local/bin/poolsync-logs && chmod 644 /home/$USER_NAME/.local/share/poolsync/poolsync-tray.png /home/$USER_NAME/.local/share/applications/com.xavdp.poolsync.desktop /home/$USER_NAME/.config/autostart/poolsync-agent.desktop"

echo "==> Pas de linger (démarrage après session RDP/XFCE)"
ssh "root@$HOST" "loginctl disable-linger $USER_NAME 2>/dev/null || true"

echo "==> Active service user"
ssh "root@$HOST" "su - $USER_NAME -c 'export XDG_RUNTIME_DIR=/run/user/\$(id -u); systemctl --user daemon-reload; systemctl --user enable --now poolsync-agent.service; sleep 1; systemctl --user status poolsync-agent.service --no-pager | head -15'"

echo "==> OK — poolsync-agent sur $HOST (display :11, autostart XFCE)"
