#!/usr/bin/env bash
set -euo pipefail

# Le heredoc réserve stdin au script exécuté dans le conteneur LXD. Cela évite
# que `lxc exec` avale la suite quand ce fichier est lui-même envoyé par ssh.
lxc exec poolsync-test -- bash -se <<'LXD_SCRIPT'
set -euo pipefail

podman network exists poolsync-net
podman image exists localhost/bench-xubuntu:24.04
podman container exists neko-desk-b

mkdir -p /srv/neko/instances/desk-c/data /srv/neko/run
cp /srv/neko/keys/authorized_keys /srv/neko/instances/desk-c/data/authorized_keys
if podman container exists neko-desk-c 2>/dev/null; then
  podman rm -f neko-desk-c >/dev/null
fi
podman run -d --name neko-desk-c --hostname desk-c --network poolsync-net \
  --shm-size 1g --restart unless-stopped \
  -v /srv/neko/instances/desk-c/data:/data:ro \
  localhost/bench-xubuntu:24.04 >/dev/null

desk_c_ready=0
for _ in $(seq 1 45); do
  if podman exec -u zaza -e DISPLAY=:99 neko-desk-c \
      xdotool getdisplaygeometry >/dev/null 2>&1 \
    && podman exec neko-desk-c pgrep -x xfce4-panel >/dev/null 2>&1; then
    desk_c_ready=1
    break
  fi
  sleep 1
done
if [[ "$desk_c_ready" != 1 ]]; then
  echo "desk-c: X11/XFCE non prêt après 45 s" >&2
  podman logs --tail 80 neko-desk-c >&2 || true
  exit 1
fi

C="$(podman inspect -f '{{(index .NetworkSettings.Networks "poolsync-net").IPAddress}}' neko-desk-c)"
B="$(podman inspect -f '{{(index .NetworkSettings.Networks "poolsync-net").IPAddress}}' neko-desk-b)"
TOKEN="$(sed -n 's/^POOLSYNC_TOKEN=//p' /srv/poolsync/hub.env)"
if [[ -z "$C" || -z "$B" || -z "$TOKEN" ]]; then
  echo "desk-c: IP desk-b/desk-c ou token du hub manquant" >&2
  exit 1
fi
geometry="$(podman exec -u zaza -e DISPLAY=:99 neko-desk-c xdotool getdisplaygeometry)"
xfce_count="$(podman exec neko-desk-c pgrep -c xfce4-panel || true)"
echo "== desk-c ip=$C ; X :99 = $geometry ; xfce=${xfce_count:-0}"

start_relay() {
  local listen_port="$1"
  local target_port="$2"
  local pid_file="/srv/neko/run/desk-c-socat-${listen_port}.pid"
  if [[ -r "$pid_file" ]]; then
    kill "$(cat "$pid_file")" 2>/dev/null || true
  fi
  # Nettoie aussi les relais créés par l'ancienne version, sans faire
  # correspondre la ligne de commande du shell qui exécute ce script.
  pkill -f "[s]ocat TCP-LISTEN:${listen_port}," 2>/dev/null || true
  nohup socat "TCP-LISTEN:${listen_port},bind=0.0.0.0,reuseaddr,fork" \
    "TCP:${C}:${target_port}" >"${pid_file%.pid}.log" 2>&1 &
  echo "$!" > "$pid_file"
}

start_relay 9083 6080
start_relay 3224 22
sleep 1
for port in 9083 3224; do
  if ! ss -tln | awk '{print $4}' | grep -Eq ":${port}$"; then
    echo "desk-c: relais TCP $port absent" >&2
    exit 1
  fi
done
echo "   relais : 9083 3224"

cat > /tmp/agent-desk-c.toml <<TOML
node = "desk-c"
hub_url = "ws://10.89.2.1:9470/ws"
token = "$TOKEN"
mode = "clipboard_only"
display = ":99"
clipboard_poll_ms = 100
peer_listen_port = 9472
peer_direct_clipboard = true
hub_clipboard = true
pause_clipboard_when_rdp = false

[screen]
width = 1600
height = 900

[[neighbors]]
direction = "left"
node = "desk-b"
peer_url = "ws://$B:9472/ws"
TOML

podman exec neko-desk-c install -d -o zaza -g zaza -m 700 \
  /home/zaza/.local/bin /home/zaza/.config/poolsync \
  /home/zaza/.local/share/poolsync /tmp/runtime-zaza
podman cp /srv/poolsync/poolsync-agent neko-desk-c:/home/zaza/.local/bin/poolsync-agent
podman cp /srv/poolsync/poolsync-tray.png neko-desk-c:/home/zaza/.local/share/poolsync/poolsync-tray.png
podman cp /tmp/agent-desk-c.toml neko-desk-c:/home/zaza/.config/poolsync/agent.toml
podman exec neko-desk-c chown -R zaza:zaza /home/zaza/.local /home/zaza/.config
podman exec neko-desk-c chmod 755 /home/zaza/.local/bin/poolsync-agent

# Remplace le bloc desk-c au lieu de seulement vérifier sa présence : après
# recréation du conteneur, son IP Podman peut avoir changé.
podman exec -i -e DESK_C_IP="$C" neko-desk-b python3 - <<'PYTHON'
import os
import re
from pathlib import Path

path = Path("/home/zaza/.config/poolsync/agent.toml")
parts = path.read_text().split("[[neighbors]]")
head = parts[0].rstrip()
neighbors = []
for raw_block in parts[1:]:
    block = raw_block.strip()
    if re.search(r'^node\s*=\s*"desk-c"\s*$', block, re.MULTILINE):
        continue
    if block:
        neighbors.append(block)
neighbors.append(
    'direction = "right"\n'
    'node = "desk-c"\n'
    f'peer_url = "ws://{os.environ["DESK_C_IP"]}:9472/ws"'
)
rendered = head + "\n\n" + "\n\n".join(
    "[[neighbors]]\n" + block for block in neighbors
) + "\n"
path.write_text(rendered)
PYTHON
podman exec neko-desk-b chown zaza:zaza /home/zaza/.config/poolsync/agent.toml
podman exec neko-desk-b install -d -o zaza -g zaza -m 700 /tmp/runtime-zaza

podman exec neko-desk-c pkill -u zaza -x poolsync-agent 2>/dev/null || true
podman exec neko-desk-b pkill -u zaza -x poolsync-agent 2>/dev/null || true
sleep 1
podman exec -d -u zaza -e HOME=/home/zaza -e USER=zaza -e DISPLAY=:99 \
  -e XDG_RUNTIME_DIR=/tmp/runtime-zaza -e RUST_LOG=info neko-desk-c \
  bash -c 'exec /home/zaza/.local/bin/poolsync-agent --config /home/zaza/.config/poolsync/agent.toml >> /tmp/poolsync-agent.log 2>&1'
podman exec -d -u zaza -e HOME=/home/zaza -e USER=zaza -e DISPLAY=:99.0 \
  -e XAUTHORITY=/data/xauthority -e XDG_RUNTIME_DIR=/tmp/runtime-zaza \
  -e RUST_LOG=info neko-desk-b \
  bash -c 'exec /home/zaza/.local/bin/poolsync-agent --config /home/zaza/.config/poolsync/agent.toml >> /tmp/poolsync-agent.log 2>&1'

for container in neko-desk-b neko-desk-c; do
  agent_ready=0
  for _ in $(seq 1 20); do
    if podman exec "$container" pgrep -u zaza -x poolsync-agent >/dev/null 2>&1; then
      agent_ready=1
      break
    fi
    sleep 1
  done
  if [[ "$agent_ready" != 1 ]]; then
    echo "$container: agent PoolSync non démarré" >&2
    podman exec "$container" tail -n 40 /tmp/poolsync-agent.log >&2 || true
    exit 1
  fi
done

echo "== desk-c agent :"
podman exec neko-desk-c tail -n 4 /tmp/poolsync-agent.log \
  | sed 's/.*poolsync_agent:*//' | cut -c1-100 | sed 's/^/   /'
status="$(curl -fsS -H "Authorization: Bearer $TOKEN" \
  http://127.0.0.1:9470/api/status)"
echo "== hub : $(python3 -c 'import json,sys; d=json.load(sys.stdin); print(", ".join(n["name"]+(" en ligne" if n["online"] else " HORS LIGNE") for n in d["nodes"]))' <<<"$status")"
LXD_SCRIPT
