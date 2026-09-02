lxc exec poolsync-test -- bash -c '
X="-u zaza -e HOME=/home/zaza -e DISPLAY=:99.0 -e XAUTHORITY=/data/xauthority"
A=neko-desk-a
echo "== outils capture : $(podman exec $A sh -c "for t in scrot import xwd xfce4-screenshooter; do command -v \$t >/dev/null && printf \"%s \" \$t; done")"
podman exec $A sh -c "command -v scrot >/dev/null || (nohup sh -c \"DEBIAN_FRONTEND=noninteractive apt-get install -y -qq scrot >/tmp/scrot.log 2>&1\" >/dev/null 2>&1 &)"
echo "== fenêtres visibles sur desk-a :"
podman exec $X $A xdotool search --onlyvisible --name "." getwindowname 2>/dev/null | sort -u | head -8 | sed "s/^/   /"
echo "== fenêtre active : $(podman exec $X $A xdotool getactivewindow getwindowname 2>&1)"
echo "== re-test frappe avec focus explicite + marqueur"
w=$(podman exec $X $A xdotool search --onlyvisible --name BANC-A 2>/dev/null | head -1)
echo "   id BANC-A = ${w:-introuvable}"
if [ -n "$w" ]; then
  podman exec $X $A xdotool windowactivate --sync $w windowfocus --sync $w
  sleep 1
  podman exec $X $A xdotool type --window $w --delay 40 "touch /tmp/typed-ok; printf %s BANC-TEXTE-2 | xclip -selection clipboard -i"
  podman exec $X $A xdotool key --window $w Return
  sleep 4
  echo "   marqueur : $(podman exec $A ls /tmp/typed-ok 2>/dev/null || echo ABSENT)"
  echo "   desk-a CLIPBOARD = [$(podman exec $X $A xclip -selection clipboard -o 2>/dev/null)]"
  echo "   desk-b CLIPBOARD = [$(podman exec $X neko-desk-b xclip -selection clipboard -o 2>/dev/null)]"
fi
' </dev/null
