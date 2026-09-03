#!/usr/bin/env bash
# Simule des écrans supplémentaires sur un bureau du banc (pilote Xorg dummy).
# À lancer DANS le conteneur poolsync-test.
#
#   simulate-screens.sh <desk> mono        1 écran 1600x900
#   simulate-screens.sh <desk> hdmi        + 1920x1080 à droite (comme asus : eDP + HDMI)
#   simulate-screens.sh <desk> triple      1280x1024 | 1920x1080 | 1600x900 (primaire à droite)
#   simulate-screens.sh <desk> no-primary  aucun moniteur primaire RandR
#   simulate-screens.sh <desk> show        état courant
#
# Le pilote dummy expose DUMMY0..DUMMY15 : on en allume autant qu'on veut, à chaud.
# `cvt` est absent des images du banc, d'où les modelines écrites en dur.
set -u
desk="${1:?usage: simulate-screens.sh <desk-a|desk-b|desk-c> <mono|hdmi|triple|no-primary|show>}"
mode="${2:-show}"
c="neko-$desk"
if [ "$desk" = desk-c ]; then X="-u zaza -e DISPLAY=:99"; else X="-u zaza -e DISPLAY=:99.0 -e XAUTHORITY=/data/xauthority"; fi
x() { podman exec $X "$c" "$@"; }

modeline_for() {
  case "$1" in
    1920x1080) echo '173.00 1920 2048 2248 2576 1080 1083 1088 1120 -hsync +vsync' ;;
    1280x1024) echo '109.00 1280 1368 1496 1712 1024 1027 1034 1063 -hsync +vsync' ;;
    1366x768)  echo '85.25 1366 1440 1576 1784 768 771 781 798 -hsync +vsync' ;;
    1600x900)  echo '118.25 1600 1696 1856 2112 900 903 908 934 -hsync +vsync' ;;
    *) return 1 ;;
  esac
}

mode_add() { # <sortie> <LxH> : crée le mode au besoin, l'attache, renvoie son nom
  local out="$1" geom="$2" name ml
  name="${geom}_60"
  ml="$(modeline_for "$geom")" || { echo "géométrie inconnue: $geom" >&2; return 1; }
  x sh -c "xrandr --newmode '$name' $ml 2>/dev/null; xrandr --addmode $out '$name' 2>/dev/null" >/dev/null 2>&1
  printf '%s' "$name"
}

case "$mode" in
  mono)
    for o in DUMMY1 DUMMY2; do x xrandr --output "$o" --off 2>/dev/null; done
    x xrandr --fb 1600x900
    x xrandr --output DUMMY0 --mode 1600x900 --pos 0x0 --primary
    ;;
  hdmi)
    m1="$(mode_add DUMMY1 1920x1080)"
    x xrandr --fb 3520x1080
    x xrandr --output DUMMY0 --pos 0x0 --primary
    x xrandr --output DUMMY1 --mode "$m1" --pos 1600x0
    ;;
  triple)
    m1="$(mode_add DUMMY1 1920x1080)"
    m2="$(mode_add DUMMY2 1280x1024)"
    x xrandr --fb 4800x1080
    x xrandr --output DUMMY2 --mode "$m2" --pos 0x0
    x xrandr --output DUMMY1 --mode "$m1" --pos 1280x0
    x xrandr --output DUMMY0 --pos 3200x0 --primary
    ;;
  no-primary)
    # RandR n'a pas de « --noprimary » : désigner une sortie éteinte revient à
    # n'avoir aucun primaire actif. C'est le cas qui piège PoolSync.
    x xrandr --output DUMMY3 --primary 2>/dev/null || true
    ;;
  show) : ;;
  *) echo "mode inconnu: $mode" >&2; exit 1 ;;
esac

echo "== $desk :"
x xrandr --listmonitors 2>&1 | sed 's/^/   /'
x xrandr 2>&1 | grep -E "^(Screen|[A-Za-z0-9]+ connected)" | sed 's/^/   /'
