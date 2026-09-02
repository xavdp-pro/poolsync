lxc exec poolsync-test -- bash -c '
for d in desk-a desk-b; do
  c=neko-$d
  podman exec $c pkill -u zaza -x poolsync-agent 2>/dev/null; sleep 1
  podman exec -d -u zaza -e HOME=/home/zaza -e USER=zaza -e DISPLAY=:99.0 -e XAUTHORITY=/data/xauthority -e XDG_RUNTIME_DIR=/tmp/runtime-zaza -e RUST_LOG=info $c bash -c "mkdir -p /tmp/runtime-zaza; exec /home/zaza/.local/bin/poolsync-agent --config /home/zaza/.config/poolsync/agent.toml >> /tmp/poolsync-agent.log 2>&1"
done
sleep 8
for d in desk-a desk-b; do
  c=neko-$d
  echo "== $d : agent pid=$(podman exec $c pgrep -u zaza -x poolsync-agent | head -1)"
  podman exec $c tail -n 6 /tmp/poolsync-agent.log 2>/dev/null | sed "s/.*poolsync_agent:*//" | cut -c1-110 | sed "s/^/   /"
done
echo "== hub : $(curl -s http://127.0.0.1:9470/api/status?token=$(sed -n "s/^POOLSYNC_TOKEN=//p" /srv/poolsync/hub.env) | python3 -c "import sys,json; d=json.load(sys.stdin); print(\", \".join(n[\"name\"]+(\" (en ligne)\" if n[\"online\"] else \" (hors ligne)\") for n in d[\"nodes\"]))" 2>/dev/null)"
' </dev/null
