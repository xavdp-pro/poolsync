lxc exec poolsync-test -- bash -c '
set -u
mkdir -p /srv/neko/instances/desk-c/data && cp /srv/neko/keys/authorized_keys /srv/neko/instances/desk-c/data/authorized_keys
podman container exists neko-desk-c 2>/dev/null && podman rm -f neko-desk-c >/dev/null
podman run -d --name neko-desk-c --hostname desk-c --network poolsync-net --shm-size 1g --restart unless-stopped \
  -v /srv/neko/instances/desk-c/data:/data:ro localhost/bench-xubuntu:24.04 >/dev/null
sleep 12
C=$(podman inspect -f "{{(index .NetworkSettings.Networks \"poolsync-net\").IPAddress}}" neko-desk-c)
echo "== desk-c ip=$C ; X :99 = $(podman exec -u zaza -e DISPLAY=:99 neko-desk-c xdotool getdisplaygeometry 2>&1) ; xfce=$(podman exec neko-desk-c pgrep -c xfce4-panel 2>/dev/null || echo 0)"
# relais socat : web noVNC 9083 -> 6080, ssh 3224 -> 22
mkdir -p /srv/neko/run
for m in "9083 6080" "3224 22"; do set -- $m; pkill -f "TCP-LISTEN:$1," 2>/dev/null; nohup socat TCP-LISTEN:$1,bind=0.0.0.0,reuseaddr,fork TCP:$C:$2 >/dev/null 2>&1 & done
sleep 1; echo "   relais : $(ss -tln | awk "{print \$4}" | grep -E ":(9083|3224)$" | tr "\n" " ")"
# agent PoolSync dans desk-c (voisin gauche = desk-b) ; desk-b gagne un voisin droit = desk-c
TOKEN=$(sed -n "s/^POOLSYNC_TOKEN=//p" /srv/poolsync/hub.env)
B=$(podman inspect -f "{{(index .NetworkSettings.Networks \"poolsync-net\").IPAddress}}" neko-desk-b)
cat > /tmp/agent-desk-c.toml <<T
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
T
podman exec neko-desk-c bash -c "mkdir -p /home/zaza/.local/bin /home/zaza/.config/poolsync /home/zaza/.local/share/poolsync"
podman cp /srv/poolsync/poolsync-agent neko-desk-c:/home/zaza/.local/bin/poolsync-agent
podman cp /srv/poolsync/poolsync-tray.png neko-desk-c:/home/zaza/.local/share/poolsync/poolsync-tray.png
podman cp /tmp/agent-desk-c.toml neko-desk-c:/home/zaza/.config/poolsync/agent.toml
podman exec neko-desk-c bash -c "chown -R zaza:zaza /home/zaza/.local /home/zaza/.config; chmod 755 /home/zaza/.local/bin/poolsync-agent"
podman exec -d -u zaza -e HOME=/home/zaza -e USER=zaza -e DISPLAY=:99 -e XDG_RUNTIME_DIR=/tmp/runtime-zaza -e RUST_LOG=info neko-desk-c bash -c "exec /home/zaza/.local/bin/poolsync-agent --config /home/zaza/.config/poolsync/agent.toml >> /tmp/poolsync-agent.log 2>&1"
# desk-b : ajouter le voisin droit desk-c et relancer son agent
podman exec neko-desk-b bash -c "grep -q \"node = \\\"desk-c\\\"\" /home/zaza/.config/poolsync/agent.toml || printf \"\n[[neighbors]]\ndirection = \\\"right\\\"\nnode = \\\"desk-c\\\"\npeer_url = \\\"ws://$C:9472/ws\\\"\n\" >> /home/zaza/.config/poolsync/agent.toml"
podman exec neko-desk-b pkill -u zaza -x poolsync-agent; sleep 1
podman exec -d -u zaza -e HOME=/home/zaza -e USER=zaza -e DISPLAY=:99.0 -e XAUTHORITY=/data/xauthority -e XDG_RUNTIME_DIR=/tmp/runtime-zaza -e RUST_LOG=info neko-desk-b bash -c "exec /home/zaza/.local/bin/poolsync-agent --config /home/zaza/.config/poolsync/agent.toml >> /tmp/poolsync-agent.log 2>&1"
sleep 8
echo "== desk-c agent :"; podman exec neko-desk-c tail -n 4 /tmp/poolsync-agent.log | sed "s/.*poolsync_agent:*//" | cut -c1-100 | sed "s/^/   /"
echo "== hub : $(curl -s "http://127.0.0.1:9470/api/status?token=$TOKEN" | python3 -c "import sys,json; d=json.load(sys.stdin); print(\", \".join(n[\"name\"]+(\" en ligne\" if n[\"online\"] else \" HORS LIGNE\") for n in d[\"nodes\"]))")"
' </dev/null
