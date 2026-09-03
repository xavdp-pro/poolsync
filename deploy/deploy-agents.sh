#!/usr/bin/env bash
# Déploie target/release/poolsync-agent sur asus + acer + gbs-p3 (glibc >= 2.39).
# gbs-p2 est en glibc 2.36 : y compiler sur place (voir deploy/bench/README.md).
set -uo pipefail
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BIN="$ROOT/target/release/poolsync-agent"
[ -x "$BIN" ] || { echo "binaire absent : $BIN" >&2; exit 1; }
SSHOPT="${POOLSYNC_SSHOPT:--o BatchMode=yes}"

echo "== asus"
systemctl --user stop poolsync-watchdog.timer poolsync-watchdog.service poolsync-agent 2>/dev/null
pkill -u "$USER" -x poolsync-agent 2>/dev/null; sleep 2
cp "$BIN" "$HOME/.local/bin/poolsync-agent" && echo "  installé"
systemctl --user start poolsync-agent poolsync-watchdog.timer; sleep 2
echo "  $(systemctl --user is-active poolsync-agent)"

echo "== acer"
ssh $SSHOPT zaza@acer 'systemctl --user stop poolsync-watchdog.timer poolsync-watchdog.service poolsync-agent 2>/dev/null; pkill -u zaza -x poolsync-agent 2>/dev/null; sleep 2; true'
scp -q $SSHOPT "$BIN" zaza@acer:/tmp/psa
ssh $SSHOPT zaza@acer 'mv /tmp/psa ~/.local/bin/poolsync-agent && chmod +x ~/.local/bin/poolsync-agent && systemctl --user start poolsync-agent poolsync-watchdog.timer && sleep 2 && echo "  $(systemctl --user is-active poolsync-agent)"'

echo "== gbs-p3"
scp -q $SSHOPT "$BIN" root@gbs-p3:/tmp/psa
ssh $SSHOPT root@gbs-p3 'uid=$(id -u zaza); R="XDG_RUNTIME_DIR=/run/user/$uid DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$uid/bus"; env $R runuser -u zaza -- systemctl --user stop poolsync-watchdog.timer poolsync-watchdog.service poolsync-agent 2>/dev/null; sleep 1; pkill -u zaza -x poolsync-agent 2>/dev/null; sleep 2; install -o zaza -g zaza -m 755 /tmp/psa /home/zaza/.local/bin/poolsync-agent && rm -f /tmp/psa; env $R runuser -u zaza -- systemctl --user start poolsync-agent poolsync-watchdog.timer; sleep 2; env $R runuser -u zaza -- systemctl --user is-active poolsync-agent | sed "s/^/  /"'
