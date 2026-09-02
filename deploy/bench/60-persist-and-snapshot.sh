lxc exec poolsync-test -- bash -c '
cat > /srv/poolsync/bench-agents.sh <<"B"
#!/usr/bin/env bash
# Démarre les bureaux du banc puis les agents PoolSync dedans (idempotent).
cd /srv/neko
for d in desk-a desk-b desk-c; do
  [ -d instances/$d ] || continue
  podman container exists neko-$d 2>/dev/null && podman container inspect -f "{{.State.Running}}" neko-$d | grep -q true || ./bin/neko-desk start $d >/dev/null 2>&1
done
sleep 10
for d in desk-a desk-b desk-c; do
  c=neko-$d; podman container exists $c 2>/dev/null || continue
  podman exec $c pgrep -u zaza -x poolsync-agent >/dev/null 2>&1 && continue
  podman exec -d -u zaza -e HOME=/home/zaza -e USER=zaza -e DISPLAY=:99.0 -e XAUTHORITY=/data/xauthority -e XDG_RUNTIME_DIR=/tmp/runtime-zaza -e RUST_LOG=info $c bash -c "mkdir -p /tmp/runtime-zaza; exec /home/zaza/.local/bin/poolsync-agent --config /home/zaza/.config/poolsync/agent.toml >> /tmp/poolsync-agent.log 2>&1"
done
B
chmod +x /srv/poolsync/bench-agents.sh
cat > /etc/systemd/system/poolsync-bench.service <<U
[Unit]
Description=PoolSync bench — bureaux Neko + agents
After=network-online.target poolsync-hub.service
Wants=poolsync-hub.service
[Service]
Type=oneshot
RemainAfterExit=yes
ExecStart=/srv/poolsync/bench-agents.sh
[Install]
WantedBy=multi-user.target
U
cat > /etc/systemd/system/poolsync-bench-watch.timer <<T
[Unit]
Description=Relance les agents du banc s ils sont tombés
[Timer]
OnBootSec=2min
OnUnitActiveSec=2min
[Install]
WantedBy=timers.target
T
cat > /etc/systemd/system/poolsync-bench-watch.service <<W
[Unit]
Description=Vérifie les agents du banc
[Service]
Type=oneshot
ExecStart=/srv/poolsync/bench-agents.sh
W
systemctl daemon-reload; systemctl enable poolsync-bench.service poolsync-bench-watch.timer >/dev/null 2>&1; systemctl start poolsync-bench-watch.timer
echo "  unités : hub=$(systemctl is-enabled poolsync-hub) bench=$(systemctl is-enabled poolsync-bench) watch=$(systemctl is-active poolsync-bench-watch.timer)"
' </dev/null
lxc snapshot poolsync-test base-2desks </dev/null 2>&1 | tail -1; echo "  snapshots : $(lxc info poolsync-test </dev/null 2>/dev/null | grep -A3 '^Snapshots' | grep -oE 'base[a-z0-9-]*' | tr '\n' ' ')"
