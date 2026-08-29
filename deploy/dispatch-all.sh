#!/usr/bin/env bash
# deploy/dispatch-all.sh
# Compile 1 seule fois sur asus (glibc 2.39) et déploie le binaire universel sur acer et inspiron (glibc 2.43).
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
source "$(dirname "$0")/load-env.sh"
TOKEN="$POOLSYNC_TOKEN"
TARGETS=("acer" "inspiron-gbs")

echo "=== [1/3] Compilation du binaire release universel sur asus ==="
source "${HOME}/.cargo/env" 2>/dev/null || true
(cd "$ROOT" && cargo build --release -p poolsync-agent)

echo "=== [2/3] Installation locale sur asus ==="
POOLSYNC_TOKEN="$TOKEN" NO_APT=1 "$ROOT/deploy/install-agent-local.sh" asus
systemctl --user stop poolsync-agent
cp "$ROOT/target/release/poolsync-agent" ~/.local/bin/poolsync-agent
systemctl --user start poolsync-agent
echo "✔ asus à jour."

echo "=== [3/3] Déploiement du binaire universel sur acer et inspiron ==="
for TARGET in "${TARGETS[@]}"; do
  NODE_NAME="$(echo "$TARGET" | cut -d'-' -f1)"
  echo "--> Déploiement vers $TARGET ($NODE_NAME)..."

  # Arrêt préventif du service distant pour libérer le fichier binaire
  ssh "zaza@$TARGET" "systemctl --user stop poolsync-agent.service 2>/dev/null || true"
  ssh "zaza@$TARGET" "mkdir -p ~/.local/bin ~/.config/poolsync ~/.local/share/poolsync ~/.config/systemd/user ~/.config/autostart"
  scp "$ROOT/target/release/poolsync-agent" "zaza@$TARGET:~/.local/bin/poolsync-agent"
  scp "$ROOT/deploy/poolsync-agent-launch.sh" "zaza@$TARGET:~/.local/bin/poolsync-agent-launch.sh"
  scp "$ROOT/deploy/poolsync-pick-session.sh" "zaza@$TARGET:~/.local/bin/poolsync-pick-session.sh"
  scp "$ROOT/deploy/poolsync-logs.sh" "zaza@$TARGET:~/.local/bin/poolsync-logs"
  scp "$ROOT/deploy/poolsync-ctl.sh" "zaza@$TARGET:~/.local/bin/poolsync-ctl"
  scp "$ROOT/deploy/read-image-clipboard.py" "zaza@$TARGET:~/.local/bin/read-image-clipboard.py"
  scp "$ROOT/deploy/write-image-clipboard.py" "zaza@$TARGET:~/.local/bin/write-image-clipboard.py"
  scp "$ROOT/deploy/poolsync-watchdog.sh" "zaza@$TARGET:~/.local/bin/poolsync-watchdog.sh"
  scp "$ROOT/poolsync-agent/icons/poolsync-tray.png" "zaza@$TARGET:~/.local/share/poolsync/poolsync-tray.png"
  scp "$ROOT/deploy/systemd/poolsync-agent.service" "zaza@$TARGET:~/.config/systemd/user/poolsync-agent.service"
  scp "$ROOT/deploy/autostart/poolsync-agent.desktop" "zaza@$TARGET:~/.config/autostart/poolsync-agent.desktop"
  scp "$ROOT/deploy/com.xavdp.poolsync.desktop" "zaza@$TARGET:~/.local/share/applications/com.xavdp.poolsync.desktop"
  scp "$ROOT/deploy/com.xavdp.poolsync-restart.desktop" "zaza@$TARGET:~/.local/share/applications/com.xavdp.poolsync-restart.desktop"
  scp "$ROOT/deploy/com.xavdp.poolsync-stop.desktop" "zaza@$TARGET:~/.local/share/applications/com.xavdp.poolsync-stop.desktop"
  scp "$ROOT/deploy/setup-poolsync-icons.sh" "zaza@$TARGET:/tmp/setup-poolsync-icons.sh"
  scp "$ROOT/deploy/setup-xfce-favorites.sh" "zaza@$TARGET:/tmp/setup-xfce-favorites.sh"
  ssh "zaza@$TARGET" "chmod +x ~/.local/bin/poolsync-ctl /tmp/setup-poolsync-icons.sh /tmp/setup-xfce-favorites.sh"
  ssh "zaza@$TARGET" "/tmp/setup-poolsync-icons.sh zaza && /tmp/setup-xfce-favorites.sh zaza"

  # Configuration spécifique au nœud avec remplacement du token
  sed "s/POOLSYNC_TOKEN_PLACEHOLDER/$TOKEN/" "$ROOT/deploy/config/agent.${NODE_NAME}.toml" | \
    ssh "zaza@$TARGET" "cat > ~/.config/poolsync/agent.toml"

  # Redémarrage du service distant sous la session utilisateur zaza
  ssh "zaza@$TARGET" "systemctl --user daemon-reload && systemctl --user restart poolsync-agent.service"
  echo "✔ $TARGET à jour."
done

echo "🎉 Déploiement universel terminé avec succès sur asus, acer et inspiron !"
