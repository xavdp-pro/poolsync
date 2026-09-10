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
SECURITY_DIR="${POOLSYNC_SECURITY_DIR:-}"
AGENT_BIN="${POOLSYNC_AGENT_BINARY:-$ROOT/target/release/poolsync-agent}"

CONFIG_SRC="$ROOT/deploy/config/agent.${NODE}.toml"
if [[ ! -f "$CONFIG_SRC" ]]; then
  echo "Config introuvable: $CONFIG_SRC" >&2
  exit 1
fi

if [[ -z "${POOLSYNC_AGENT_BINARY:-}" ]]; then
  echo "==> Build release (agent)"
  source "${HOME}/.cargo/env" 2>/dev/null || true
  (cd "$ROOT" && cargo build --release -p poolsync-agent)
else
  echo "==> Réutilise le binaire agent fourni"
fi
[[ -x "$AGENT_BIN" ]] || { echo "binaire agent absent : $AGENT_BIN" >&2; exit 1; }

echo "==> Dépendances X11 sur $HOST"
ssh "root@$HOST" 'DEBIAN_FRONTEND=noninteractive apt-get install -y -qq xclip xdotool wl-clipboard libnotify-bin python3-gi gir1.2-gtk-3.0 >/dev/null'

echo "==> Binaire + config pour $USER_NAME@$HOST"
ssh "root@$HOST" "install -d -o $USER_NAME -g $USER_NAME /home/$USER_NAME/.local/bin /home/$USER_NAME/.config/poolsync /home/$USER_NAME/.config/poolsync/tls /home/$USER_NAME/.local/share/poolsync /home/$USER_NAME/.local/share/applications /home/$USER_NAME/.config/systemd/user /home/$USER_NAME/.config/autostart"
ssh "root@$HOST" "uid=\$(id -u $USER_NAME); runuser -u $USER_NAME -- env XDG_RUNTIME_DIR=/run/user/\$uid DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/\$uid/bus systemctl --user stop poolsync-watchdog.timer poolsync-watchdog.service poolsync-agent.service 2>/dev/null || true; pkill -u $USER_NAME -x poolsync-agent 2>/dev/null || true"
scp "$AGENT_BIN" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-agent.new"
scp "$ROOT/deploy/poolsync-agent-launch.sh" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-agent-launch.sh"
scp "$ROOT/deploy/poolsync-pick-session.sh" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-pick-session.sh"
scp "$ROOT/deploy/poolsync-logs.sh" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-logs"
scp "$ROOT/deploy/poolsync-watchdog.sh" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-watchdog.sh"
scp "$ROOT/deploy/write-image-clipboard.py" "root@$HOST:/home/$USER_NAME/.local/bin/write-image-clipboard.py"
scp "$ROOT/poolsync-agent/icons/poolsync-tray.png" "root@$HOST:/home/$USER_NAME/.local/share/poolsync/poolsync-tray.png"
scp "$ROOT/deploy/com.xavdp.poolsync.desktop" "root@$HOST:/home/$USER_NAME/.local/share/applications/com.xavdp.poolsync.desktop"
ssh "root@$HOST" "mv /home/$USER_NAME/.local/bin/poolsync-agent.new /home/$USER_NAME/.local/bin/poolsync-agent && chown $USER_NAME:$USER_NAME /home/$USER_NAME/.local/bin/poolsync-agent && chmod 755 /home/$USER_NAME/.local/bin/poolsync-agent"

TMP_CFG="$(mktemp)"
TMP_BASE="${TMP_CFG}.base"
trap 'rm -f "$TMP_CFG" "$TMP_BASE"' EXIT
sed "s/POOLSYNC_TOKEN_PLACEHOLDER/$TOKEN/" "$CONFIG_SRC" > "$TMP_BASE"
if [[ -n "$SECURITY_DIR" ]]; then
  for file in "$SECURITY_DIR/ca.crt" "$SECURITY_DIR/nodes/$NODE.crt" "$SECURITY_DIR/nodes/$NODE.key" "$SECURITY_DIR/nodes/$NODE.toml"; do
    [[ -s "$file" ]] || { echo "fichier sécurité absent: $file" >&2; exit 1; }
  done
  {
    sed '/^\[screen\]/,$d' "$TMP_BASE" \
      | sed -e 's#hub_url = "ws://#hub_url = "wss://#' \
        -e 's#peer_url = "ws://#peer_url = "wss://#' \
        -e 's#peer_url_vpn = "ws://#peer_url_vpn = "wss://#' \
        -e 's/^token = .*/token = "MIGRATION_DISABLED"/'
    sed "s#POOLSYNC_TLS_DIR#/home/$USER_NAME/.config/poolsync/tls#g" \
      "$SECURITY_DIR/nodes/$NODE.toml"
    sed -n '/^\[screen\]/,$p' "$TMP_BASE" \
      | sed -e 's#peer_url = "ws://#peer_url = "wss://#' \
        -e 's#peer_url_vpn = "ws://#peer_url_vpn = "wss://#'
  } > "$TMP_CFG"
  scp "$SECURITY_DIR/ca.crt" "$SECURITY_DIR/nodes/$NODE.crt" "$SECURITY_DIR/nodes/$NODE.key" \
    "root@$HOST:/home/$USER_NAME/.config/poolsync/tls/"
  scp "$SECURITY_DIR/ca.crt" "root@$HOST:/usr/local/share/ca-certificates/poolsync-ca.crt"
  ssh "root@$HOST" "update-ca-certificates >/dev/null && chown -R $USER_NAME:$USER_NAME /home/$USER_NAME/.config/poolsync/tls && chmod 700 /home/$USER_NAME/.config/poolsync/tls && chmod 600 /home/$USER_NAME/.config/poolsync/tls/$NODE.key"
else
  mv "$TMP_BASE" "$TMP_CFG"
fi
scp "$TMP_CFG" "root@$HOST:/home/$USER_NAME/.config/poolsync/agent.toml"
scp "$ROOT/deploy/systemd/poolsync-agent.service" "root@$HOST:/home/$USER_NAME/.config/systemd/user/poolsync-agent.service"
scp "$ROOT/deploy/systemd/poolsync-watchdog.service" "root@$HOST:/home/$USER_NAME/.config/systemd/user/poolsync-watchdog.service"
scp "$ROOT/deploy/systemd/poolsync-watchdog.timer" "root@$HOST:/home/$USER_NAME/.config/systemd/user/poolsync-watchdog.timer"
scp "$ROOT/deploy/autostart/poolsync-agent.desktop" "root@$HOST:/home/$USER_NAME/.config/autostart/poolsync-agent.desktop"
ssh "root@$HOST" "chown -R $USER_NAME:$USER_NAME /home/$USER_NAME/.config/poolsync /home/$USER_NAME/.local/share/poolsync /home/$USER_NAME/.local/share/applications/com.xavdp.poolsync.desktop /home/$USER_NAME/.config/systemd/user/poolsync-agent.service /home/$USER_NAME/.config/systemd/user/poolsync-watchdog.service /home/$USER_NAME/.config/systemd/user/poolsync-watchdog.timer /home/$USER_NAME/.config/autostart/poolsync-agent.desktop /home/$USER_NAME/.local/bin/poolsync-agent-launch.sh /home/$USER_NAME/.local/bin/poolsync-pick-session.sh /home/$USER_NAME/.local/bin/poolsync-logs /home/$USER_NAME/.local/bin/poolsync-watchdog.sh /home/$USER_NAME/.local/bin/write-image-clipboard.py && chmod 755 /home/$USER_NAME/.local/bin/poolsync-agent-launch.sh /home/$USER_NAME/.local/bin/poolsync-pick-session.sh /home/$USER_NAME/.local/bin/poolsync-logs /home/$USER_NAME/.local/bin/poolsync-watchdog.sh /home/$USER_NAME/.local/bin/write-image-clipboard.py && chmod 644 /home/$USER_NAME/.local/share/poolsync/poolsync-tray.png /home/$USER_NAME/.local/share/applications/com.xavdp.poolsync.desktop /home/$USER_NAME/.config/autostart/poolsync-agent.desktop"

echo "==> Plugin Indicator XFCE (désactivé — casse le panneau si doublon)"
# scp "$ROOT/deploy/setup-xfce-indicator.sh" "root@$HOST:/tmp/setup-xfce-indicator.sh"
# ssh "root@$HOST" "chmod +x /tmp/setup-xfce-indicator.sh && /tmp/setup-xfce-indicator.sh $USER_NAME || true"

echo "==> Active service user (sans toucher Barrier)"
ssh "root@$HOST" "loginctl disable-linger $USER_NAME 2>/dev/null || true"
scp "$ROOT/deploy/poolsync-session-start.sh" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-session-start.sh"
scp "$ROOT/deploy/poolsync-enable-user.sh" "root@$HOST:/home/$USER_NAME/.local/bin/poolsync-enable-user.sh"
ssh "root@$HOST" "chmod 755 /home/$USER_NAME/.local/bin/poolsync-session-start.sh /home/$USER_NAME/.local/bin/poolsync-enable-user.sh && chown $USER_NAME:$USER_NAME /home/$USER_NAME/.local/bin/poolsync-session-start.sh /home/$USER_NAME/.local/bin/poolsync-enable-user.sh && runuser -u $USER_NAME -- /home/$USER_NAME/.local/bin/poolsync-enable-user.sh $USER_NAME"
