#!/usr/bin/env bash
# Démarre les bureaux du banc puis les agents PoolSync dedans (idempotent).
cd /srv/neko
for d in desk-a desk-b; do
  [ -d instances/$d ] || continue
  podman container inspect -f "{{.State.Running}}" neko-$d 2>/dev/null | grep -q true || ./bin/neko-desk start $d >/dev/null 2>&1
done
# desk-c (Xubuntu) est un conteneur podman simple, avec --restart unless-stopped
podman container inspect -f "{{.State.Running}}" neko-desk-c 2>/dev/null | grep -q true || podman start neko-desk-c >/dev/null 2>&1
sleep 10
for d in desk-a desk-b desk-c; do
  c=neko-$d; podman container exists $c 2>/dev/null || continue
  # sshd (StrictModes) refuse les clés si /home/zaza n appartient pas à zaza
  podman exec $c sh -c "chown zaza:zaza /home/zaza 2>/dev/null" 2>/dev/null
  podman exec $c pgrep -u zaza -x poolsync-agent >/dev/null 2>&1 && continue
  if [ "$d" = desk-c ]; then E="-e DISPLAY=:99"; else E="-e DISPLAY=:99.0 -e XAUTHORITY=/data/xauthority"; fi
  podman exec -d -u zaza -e HOME=/home/zaza -e USER=zaza $E -e XDG_RUNTIME_DIR=/tmp/runtime-zaza -e RUST_LOG=info $c bash -c "mkdir -p /tmp/runtime-zaza; exec /home/zaza/.local/bin/poolsync-agent --config /home/zaza/.config/poolsync/agent.toml >> /tmp/poolsync-agent.log 2>&1"
done
