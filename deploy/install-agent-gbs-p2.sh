#!/usr/bin/env bash
# Installe poolsync-agent pour zaza sur gbs-p2 (session xrdp XFCE).
# Presse-papiers uniquement — pas de KVM. Pas d'install pour zaza2/root.
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

echo "==> Dépendances X11 sur $HOST"
ssh "root@$HOST" 'DEBIAN_FRONTEND=noninteractive apt-get install -y -qq xclip xdotool libnotify-bin libgtk-3-0 libayatana-appindicator3-1 2>/dev/null || apt-get install -y -qq xclip xdotool libnotify-bin'

echo "==> Répertoires pour $USER_NAME@$HOST (pas les autres comptes)"
ssh "root@$HOST" "install -d -o $USER_NAME -g $USER_NAME /home/$USER_NAME/.local/bin /home/$USER_NAME/.config/poolsync /home/$USER_NAME/.local/share/poolsync /home/$USER_NAME/.local/share/applications /home/$USER_NAME/.config/systemd/user /home/$USER_NAME/.config/autostart"
ssh "root@$HOST" "su - $USER_NAME -c 'export XDG_RUNTIME_DIR=/run/user/\$(id -u); systemctl --user stop poolsync-agent.service 2>/dev/null || true'"

# Binaire asus (glibc 2.39) incompatible Debian 12 (2.36) : conserver le binaire local s'il existe.
REMOTE_BIN_OK="$(ssh "root@$HOST" "test -x /home/$USER_NAME/.local/bin/poolsync-agent && echo yes || echo no")"
if [[ "$REMOTE_BIN_OK" != "yes" ]]; then
  echo "ERREUR: pas de poolsync-agent sur $HOST et le binaire asus n'est pas déployable (glibc)." >&2
  exit 1
fi
echo "==> Binaire existant conservé (build Debian 12)"

scp "$ROOT/deploy/poolsync-agent-launch.sh" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-agent-launch.sh"
scp "$ROOT/deploy/poolsync-session-start.sh" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-session-start.sh"
scp "$ROOT/deploy/poolsync-logs.sh" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-logs"
scp "$ROOT/deploy/autostart/poolsync-agent.desktop" "root@$HOST:/home/$USER_NAME/.config/autostart/poolsync-agent.desktop"
scp "$ROOT/poolsync-agent/icons/poolsync-tray.png" "root@$HOST:/home/$USER_NAME/.local/share/poolsync/poolsync-tray.png"
scp "$ROOT/deploy/com.xavdp.poolsync.desktop" "root@$HOST:/home/$USER_NAME/.local/share/applications/com.xavdp.poolsync.desktop"
if [[ -f "$ROOT/deploy/write-image-clipboard.py" ]]; then
  scp "$ROOT/deploy/write-image-clipboard.py" "root@$HOST:/home/$USER_NAME/.local/bin/write-image-clipboard.py"
fi
if [[ -f "$ROOT/deploy/read-image-clipboard.py" ]]; then
  scp "$ROOT/deploy/read-image-clipboard.py" "root@$HOST:/home/$USER_NAME/.local/bin/read-image-clipboard.py"
fi

TMP_CFG="$(mktemp)"
trap 'rm -f "$TMP_CFG"' EXIT
sed "s/POOLSYNC_TOKEN_PLACEHOLDER/$TOKEN/" "$CONFIG_SRC" > "$TMP_CFG"
scp "$TMP_CFG" "root@$HOST:/home/$USER_NAME/.config/poolsync/agent.toml"

# Pas de DISPLAY figé : le lanceur prend la session XFCE de zaza (jamais zaza2).
ssh "root@$HOST" "cat > /home/$USER_NAME/.config/systemd/user/poolsync-agent.service" <<EOF
[Unit]
Description=PoolSync agent (clipboard only, no KVM)
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
ExecStart=%h/.local/bin/poolsync-agent-launch.sh --config %h/.config/poolsync/agent.toml
Restart=on-failure
RestartSec=5
Environment=XDG_CURRENT_DESKTOP=XFCE
Environment=GDK_BACKEND=x11

[Install]
WantedBy=default.target
EOF

ssh "root@$HOST" "chown -R $USER_NAME:$USER_NAME /home/$USER_NAME/.config/poolsync /home/$USER_NAME/.local/share/poolsync /home/$USER_NAME/.local/share/applications/com.xavdp.poolsync.desktop /home/$USER_NAME/.config/systemd/user/poolsync-agent.service /home/$USER_NAME/.config/autostart/poolsync-agent.desktop /home/$USER_NAME/.local/bin/poolsync-agent-launch.sh /home/$USER_NAME/.local/bin/poolsync-session-start.sh /home/$USER_NAME/.local/bin/poolsync-logs && chmod 755 /home/$USER_NAME/.local/bin/poolsync-agent-launch.sh /home/$USER_NAME/.local/bin/poolsync-session-start.sh /home/$USER_NAME/.local/bin/poolsync-logs && chmod 600 /home/$USER_NAME/.config/poolsync/agent.toml && chmod 644 /home/$USER_NAME/.local/share/poolsync/poolsync-tray.png /home/$USER_NAME/.local/share/applications/com.xavdp.poolsync.desktop /home/$USER_NAME/.config/autostart/poolsync-agent.desktop"

echo "==> Désactive PoolSync pour les autres comptes (zaza2, etc.)"
ssh "root@$HOST" bash -s <<'REMOTE'
set -euo pipefail
if getent passwd zaza2 >/dev/null; then
  z2uid="$(id -u zaza2)"
  runuser -u zaza2 -- env XDG_RUNTIME_DIR="/run/user/$z2uid" \
    systemctl --user disable --now poolsync-agent.service 2>/dev/null || true
  rm -f /home/zaza2/.config/autostart/poolsync-agent.desktop
fi
REMOTE

echo "==> Active service user zaza uniquement"
ssh "root@$HOST" "su - $USER_NAME -c 'export XDG_RUNTIME_DIR=/run/user/\$(id -u); systemctl --user daemon-reload; systemctl --user enable --now poolsync-agent.service; sleep 2; systemctl --user status poolsync-agent.service --no-pager | head -18'"

echo "==> OK — poolsync-agent clipboard_only sur $HOST (session $USER_NAME, pas de KVM)"
