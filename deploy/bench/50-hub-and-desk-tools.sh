set -u
# 1) ports de pilotage neko-ctl
for r in "-p tcp --dport 9181:9183"; do iptables -t nat -C PREROUTING -d 10.87.78.36 $r -j DNAT --to-destination 10.213.199.137 2>/dev/null || iptables -t nat -A PREROUTING -d 10.87.78.36 $r -j DNAT --to-destination 10.213.199.137; done
iptables -C FORWARD -d 10.213.199.137 -p tcp --dport 9181:9183 -j ACCEPT 2>/dev/null || iptables -I FORWARD -d 10.213.199.137 -p tcp --dport 9181:9183 -j ACCEPT
echo "  DNAT ctl 9181-9183: ok"
lxc exec poolsync-test -- bash -c '
# 2) hub de test (token dédié, isolé du pool de prod)
if [ ! -f /srv/poolsync/hub.env ]; then echo "POOLSYNC_TOKEN=$(openssl rand -hex 16 2>/dev/null || head -c16 /dev/urandom | xxd -p)" > /srv/poolsync/hub.env; fi
cat > /etc/systemd/system/poolsync-hub.service <<U
[Unit]
Description=PoolSync hub (banc de test)
After=network.target
[Service]
EnvironmentFile=/srv/poolsync/hub.env
ExecStart=/srv/poolsync/poolsync-hub --listen 0.0.0.0:9470 --token \${POOLSYNC_TOKEN} --topology-file /srv/poolsync/topology.json
Restart=always
[Install]
WantedBy=multi-user.target
U
systemctl daemon-reload; systemctl enable --now poolsync-hub >/dev/null 2>&1; sleep 2
echo "  hub: $(systemctl is-active poolsync-hub) — $(curl -s http://127.0.0.1:9470/health 2>/dev/null | head -c 60)"
# 3) diagnostic ssh desk-a + X11 + IPs + paquets
for d in desk-a desk-b; do
  c=neko-$d
  ip=$(podman inspect -f "{{range .NetworkSettings.Networks}}{{.IPAddress}}{{end}}" $c 2>/dev/null)
  echo "== $d ip=$ip sshd=$(podman exec $c pgrep -c sshd 2>/dev/null || echo 0) cle_asus=$(podman exec $c grep -c "AAAAC3NzaC1lZDI1NTE5AAAA" /home/zaza/.ssh/authorized_keys 2>/dev/null || echo 0)"
  podman exec -e DISPLAY=:99.0 $c xdotool getdisplaygeometry 2>&1 | sed "s/^/   X :99 : /"
  podman exec $c bash -c "nohup sh -c \"apt-get update -qq && DEBIAN_FRONTEND=noninteractive apt-get install -y -qq xclip python3-gi gir1.2-gtk-3.0 libnotify-bin libayatana-appindicator3-1 >/tmp/pkg.log 2>&1; echo done >>/tmp/pkg.log\" >/dev/null 2>&1 &"
done
echo "  installation xclip/gi lancée en arrière-plan dans les deux bureaux"
' </dev/null
