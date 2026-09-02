lxc exec poolsync-test -- bash -c '
C=neko-desk-c
podman exec $C bash -c "apt-get update -qq >/dev/null 2>&1; DEBIAN_FRONTEND=noninteractive apt-get install -y -qq dbus-x11 >/dev/null 2>&1 && echo \"   dbus-x11 installé\""
podman exec $C supervisorctl restart xfce >/dev/null 2>&1; sleep 10
echo "== desk-c : xfwm4=$(podman exec $C pgrep -c xfwm4 2>/dev/null||echo 0) panel=$(podman exec $C pgrep -c xfce4-panel 2>/dev/null||echo 0)"
XA="-u zaza -e HOME=/home/zaza -e DISPLAY=:99.0 -e XAUTHORITY=/data/xauthority"; XC="-u zaza -e HOME=/home/zaza -e DISPLAY=:99"
echo "== image desk-c -> desk-b -> desk-a"
podman exec $XC $C xfce4-screenshooter -f -c 2>/dev/null; sleep 8
hc=$(podman exec $XC $C xclip -selection clipboard -t image/png -o 2>/dev/null | sha256sum | cut -c1-12); sc=$(podman exec $XC $C xclip -selection clipboard -t image/png -o 2>/dev/null | wc -c)
echo "   desk-c : $sc octets ($hc)"
for d in b a; do h=$(podman exec $XA neko-desk-$d xclip -selection clipboard -t image/png -o 2>/dev/null | sha256sum | cut -c1-12); [ "$h" = "$hc" ] && [ "$sc" -gt 1000 ] && echo "   desk-$d : image identique — OK" || echo "   desk-$d : DIFFÉRENT ($h)"; done
podman exec $XC $C xfce4-screenshooter -f -s /tmp/desk-c.png 2>/dev/null; podman cp $C:/tmp/desk-c.png /tmp/desk-c.png 2>/dev/null && echo "   capture desk-c : $(stat -c %s /tmp/desk-c.png) o"
' </dev/null
lxc file pull poolsync-test/tmp/desk-c.png /tmp/desk-c.png </dev/null 2>/dev/null && echo "   capture rapatriée sur gbs-test"
