lxc exec poolsync-test -- bash -c '
X="-u zaza -e HOME=/home/zaza -e DISPLAY=:99.0 -e XAUTHORITY=/data/xauthority"
A=neko-desk-a; B=neko-desk-b
type_in() { # $1=container $2=window-name $3=text
  local w; w=$(podman exec $X $1 xdotool search --onlyvisible --name "$2" 2>/dev/null | head -1)
  podman exec $X $1 xdotool windowactivate --sync $w windowfocus --sync $w; sleep 0.5
  podman exec $X $1 xdotool type --window $w --delay 30 "$3"; podman exec $X $1 xdotool key --window $w Return
}
echo "== TEXTE : copie tapée sur desk-a, collage Ctrl+Shift+V sur desk-b dans un fichier"
tag="BANC-COLLAGE-$(date +%H%M%S)"
type_in $A BANC-A "printf %s $tag | xclip -selection clipboard -i"; sleep 5
podman exec $B rm -f /tmp/colle.txt
type_in $B BANC-B "cat > /tmp/colle.txt"; sleep 1
w=$(podman exec $X $B xdotool search --onlyvisible --name BANC-B | head -1)
podman exec $X $B xdotool key --window $w ctrl+shift+v; sleep 1
podman exec $X $B xdotool key --window $w Return; sleep 0.5; podman exec $X $B xdotool key --window $w ctrl+d; sleep 1
got=$(podman exec $B cat /tmp/colle.txt 2>/dev/null | tr -d "\n")
[ "$got" = "$tag" ] && echo "   COLLÉ sur desk-b : OK [$got]" || echo "   COLLÉ sur desk-b : ÉCHEC [$got]"
echo "== IMAGE : capture d écran réelle sur desk-a copiée dans le presse-papiers"
podman exec $X $A xfce4-screenshooter -f -c 2>/dev/null; sleep 6
ha=$(podman exec $X $A xclip -selection clipboard -t image/png -o 2>/dev/null | sha256sum | cut -c1-12)
sa=$(podman exec $X $A xclip -selection clipboard -t image/png -o 2>/dev/null | wc -c)
echo "   desk-a : image/png $sa octets ($ha)"
echo "   desk-b cibles : $(podman exec $X $B xclip -selection clipboard -t TARGETS -o 2>/dev/null | tr "\n" ",")"
hb=$(podman exec $X $B xclip -selection clipboard -t image/png -o 2>/dev/null | sha256sum | cut -c1-12)
sb=$(podman exec $X $B xclip -selection clipboard -t image/png -o 2>/dev/null | wc -c)
[ "$ha" = "$hb" ] && [ "$sa" -gt 1000 ] && echo "   desk-b : image identique, $sb octets — OK" || echo "   desk-b : DIFFÉRENT ($sb octets, $hb)"
echo "== captures des deux bureaux"
for d in a b; do podman exec $X neko-desk-$d xfce4-screenshooter -f -s /tmp/desk-$d.png 2>/dev/null; podman cp neko-desk-$d:/tmp/desk-$d.png /tmp/desk-$d.png 2>/dev/null; done
ls -la /tmp/desk-a.png /tmp/desk-b.png 2>/dev/null | awk "{print \"   \"\$9, \$5\" o\"}"
' </dev/null
lxc file pull poolsync-test/tmp/desk-a.png /tmp/desk-a.png </dev/null 2>/dev/null; lxc file pull poolsync-test/tmp/desk-b.png /tmp/desk-b.png </dev/null 2>/dev/null; echo "   sur gbs-test : $(ls /tmp/desk-a.png /tmp/desk-b.png 2>/dev/null | wc -l) capture(s)"
