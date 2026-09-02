lxc exec poolsync-test -- bash -c '
C=neko-desk-c
echo "== desk-c : xfwm4=$(podman exec $C pgrep -c xfwm4 2>/dev/null||echo 0) panel=$(podman exec $C pgrep -c xfce4-panel 2>/dev/null||echo 0) x11vnc=$(podman exec $C pgrep -c x11vnc 2>/dev/null||echo 0) novnc=$(podman exec $C pgrep -fc novnc_proxy 2>/dev/null||echo 0) sshd=$(podman exec $C pgrep -c sshd 2>/dev/null||echo 0)"
podman exec $C tail -n 3 /var/log/desk/xfce.log 2>/dev/null | cut -c1-100 | sed "s/^/   xfce.log: /"
XA="-u zaza -e HOME=/home/zaza -e DISPLAY=:99.0 -e XAUTHORITY=/data/xauthority"; XC="-u zaza -e HOME=/home/zaza -e DISPLAY=:99"
tag="MULTI-SAUT-$(date +%H%M%S)"
echo "== texte desk-a -> (desk-b) -> desk-c"
podman exec $XA neko-desk-a bash -c "(setsid nohup bash -c \"printf %s $tag | xclip -selection clipboard -i\" >/dev/null 2>&1 &)"; sleep 8
for d in b c; do v=$(podman exec $([ $d = c ] && echo "$XC neko-desk-c" || echo "$XA neko-desk-b") xclip -selection clipboard -t UTF8_STRING -o 2>/dev/null); [ "$v" = "$tag" ] && echo "   desk-$d : OK" || echo "   desk-$d : NON [$v]"; done
echo "== image desk-c -> desk-b -> desk-a (capture réelle sur la Xubuntu)"
podman exec $XC $C xfce4-screenshooter -f -c 2>/dev/null; sleep 8
hc=$(podman exec $XC $C xclip -selection clipboard -t image/png -o 2>/dev/null | sha256sum | cut -c1-12); sc=$(podman exec $XC $C xclip -selection clipboard -t image/png -o 2>/dev/null | wc -c)
echo "   desk-c : $sc octets ($hc)"
for d in b a; do h=$(podman exec $XA neko-desk-$d xclip -selection clipboard -t image/png -o 2>/dev/null | sha256sum | cut -c1-12); [ "$h" = "$hc" ] && [ "$sc" -gt 1000 ] && echo "   desk-$d : image identique — OK" || echo "   desk-$d : DIFFÉRENT ($h)"; done
' </dev/null
