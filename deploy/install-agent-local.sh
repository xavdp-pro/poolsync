#!/usr/bin/env bash
# Installe poolsync-agent sur la machine courante (user zaza).
# Usage: POOLSYNC_TOKEN=xxx ./install-agent-local.sh asus
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
NODE="${1:?nom noeud (ex: asus)}"
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

echo "==> Dépendances X11/GTK locales"
if command -v apt-get >/dev/null; then
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq \
    xclip xdotool libnotify-bin libgtk-3-dev libayatana-appindicator3-dev libnotify-dev libxdo-dev 2>/dev/null || \
  sudo DEBIAN_FRONTEND=noninteractive apt-get install -y -qq xclip xdotool libnotify-bin
fi

BIN_DIR="/home/$USER_NAME/.local/bin"
CFG_DIR="/home/$USER_NAME/.config/poolsync"
ICON_DIR="/home/$USER_NAME/.local/share/poolsync"
APP_DIR="/home/$USER_NAME/.local/share/applications"
SVC_DIR="/home/$USER_NAME/.config/systemd/user"
AUTO_DIR="/home/$USER_NAME/.config/autostart"

mkdir -p "$BIN_DIR" "$CFG_DIR" "$ICON_DIR" "$APP_DIR" "$SVC_DIR"
rm -f "$AUTO_DIR/poolsync-agent.desktop"
install -m 755 "$ROOT/target/release/poolsync-agent" "$BIN_DIR/poolsync-agent"
install -m 755 "$ROOT/deploy/poolsync-agent-launch.sh" "$BIN_DIR/poolsync-agent-launch.sh"
install -m 755 "$ROOT/deploy/poolsync-logs.sh" "$BIN_DIR/poolsync-logs"
install -m 644 "$ROOT/poolsync-agent/icons/poolsync-tray.png" "$ICON_DIR/poolsync-tray.png"
install -m 644 "$ROOT/deploy/com.xavdp.poolsync.desktop" "$APP_DIR/com.xavdp.poolsync.desktop"
sed "s/POOLSYNC_TOKEN_PLACEHOLDER/$TOKEN/" "$CONFIG_SRC" > "$CFG_DIR/agent.toml"
cp "$ROOT/deploy/systemd/poolsync-agent.service" "$SVC_DIR/poolsync-agent.service"

echo "==> Plugin Indicator XFCE (désactivé — casse le panneau si doublon)"
# bash "$ROOT/deploy/setup-xfce-indicator.sh" "$USER_NAME" || true

echo "==> Active service user"
export XDG_RUNTIME_DIR="/run/user/$(id -u $USER_NAME)"
systemctl --user disable --now now3pool-agent.service 2>/dev/null || true
systemctl --user daemon-reload
systemctl --user enable --now poolsync-agent.service
systemctl --user status poolsync-agent.service --no-pager | head -12
