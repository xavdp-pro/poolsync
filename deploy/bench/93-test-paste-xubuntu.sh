lxc exec poolsync-test -- bash -c '
cd /srv/xubuntu && grep -q dbus-x11 Containerfile || sed -i "s/ supervisor openssh-server/ dbus-x11 supervisor openssh-server/" Containerfile
grep -c dbus-x11 Containerfile | sed "s/^/   Containerfile dbus-x11: /"
nohup podman build -t localhost/bench-xubuntu:24.04 . > /tmp/xubuntu-build.log 2>&1 &
XA="-u zaza -e HOME=/home/zaza -e DISPLAY=:99.0 -e XAUTHORITY=/data/xauthority"; XC="-u zaza -e HOME=/home/zaza -e DISPLAY=:99"
tag="COLLE-XUBUNTU-$(date +%H%M%S)"
echo "== texte copié sur desk-a, collé par Ctrl+Shift+V dans un terminal Xubuntu (desk-c)"
podman exec $XA neko-desk-a bash -c "(setsid nohup bash -c \"printf %s $tag | xclip -selection clipboard -i\" >/dev/null 2>&1 &)"; sleep 8
podman exec -d $XC neko-desk-c xfce4-terminal --title=BANC-C; sleep 4
w=$(podman exec $XC neko-desk-c xdotool search --onlyvisible --name BANC-C 2>/dev/null | head -1); echo "   fenêtre BANC-C : ${w:-introuvable}"
podman exec $XC neko-desk-c xdotool windowactivate --sync $w windowfocus --sync $w; sleep 0.5
podman exec neko-desk-c rm -f /tmp/colle.txt
podman exec $XC neko-desk-c xdotool type --window $w --delay 30 "cat > /tmp/colle.txt"; podman exec $XC neko-desk-c xdotool key --window $w Return; sleep 1
podman exec $XC neko-desk-c xdotool key --window $w ctrl+shift+v; sleep 1; podman exec $XC neko-desk-c xdotool key --window $w Return; sleep 0.5; podman exec $XC neko-desk-c xdotool key --window $w ctrl+d; sleep 1
got=$(podman exec neko-desk-c cat /tmp/colle.txt 2>/dev/null | tr -d "\n"); [ "$got" = "$tag" ] && echo "   COLLÉ sur desk-c : OK [$got]" || echo "   COLLÉ sur desk-c : ÉCHEC [$got]"
' </dev/null
