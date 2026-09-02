lxc exec poolsync-test -- bash -c '
set -u
echo "== relais socat dans le CT :"; ss -tlnp 2>/dev/null | awk "{print \$4}" | grep -E ":(3222|3223|9081|9082|9181|9182)$" | tr "\n" " "; echo
# réseau partagé entre les deux bureaux (compose isole chaque projet)
podman network exists poolsync-net 2>/dev/null || podman network create poolsync-net >/dev/null
for d in desk-a desk-b; do podman network connect poolsync-net neko-$d 2>/dev/null || true; done
A=$(podman inspect -f "{{(index .NetworkSettings.Networks \"poolsync-net\").IPAddress}}" neko-desk-a)
B=$(podman inspect -f "{{(index .NetworkSettings.Networks \"poolsync-net\").IPAddress}}" neko-desk-b)
GW=$(podman network inspect poolsync-net -f "{{(index .Subnets 0).Gateway}}")
echo "== poolsync-net : desk-a=$A desk-b=$B passerelle(CT)=$GW"
TOKEN=$(sed -n "s/^POOLSYNC_TOKEN=//p" /srv/poolsync/hub.env)
mk() { # $1=desk $2=self_ip $3=peer_name $4=peer_ip
cat > /tmp/agent-$1.toml <<T
node = "$1"
hub_url = "ws://$GW:9470/ws"
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
direction = "right"
node = "$3"
peer_url = "ws://$4:9472/ws"
T
}
mk desk-a $A desk-b $B; mk desk-b $B desk-a $A
for d in desk-a desk-b; do
  c=neko-$d
  podman exec $c bash -c "mkdir -p /home/zaza/.local/bin /home/zaza/.config/poolsync /home/zaza/.local/share/poolsync"
  podman cp /srv/poolsync/poolsync-agent $c:/home/zaza/.local/bin/poolsync-agent
  podman cp /srv/poolsync/poolsync-tray.png $c:/home/zaza/.local/share/poolsync/poolsync-tray.png
  podman cp /tmp/agent-$d.toml $c:/home/zaza/.config/poolsync/agent.toml
  podman exec $c bash -c "chown -R zaza:zaza /home/zaza/.local /home/zaza/.config; chmod 755 /home/zaza/.local/bin/poolsync-agent"
  for i in $(seq 1 60); do podman exec $c grep -q done /tmp/pkg.log 2>/dev/null && break; sleep 5; done
  echo "  $d : xclip=$(podman exec $c sh -c "command -v xclip >/dev/null && echo ok || echo ABSENT") glibc=$(podman exec $c ldd --version | head -1 | awk "{print \$NF}")"
done
' </dev/null
