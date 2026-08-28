#!/usr/bin/env bash
# Add PoolSync launchers to whiskermenu favorites (XFCE applications menu).
set -euo pipefail

USER_NAME="${1:-zaza}"
HOME_DIR="$(getent passwd "$USER_NAME" | cut -d: -f6)"
XML="${HOME_DIR}/.config/xfce4/xfconf/xfce-perchannel-xml/xfce4-panel.xml"
FAV_IDS=(
  "com.xavdp.poolsync.desktop"
  "com.xavdp.poolsync-restart.desktop"
  "com.xavdp.poolsync-stop.desktop"
)

run_as() {
  if [[ "$(id -un)" == "$USER_NAME" ]]; then
    bash -lc "$1"
  else
    su - "$USER_NAME" -c "$1"
  fi
}

patch_xml_favorites() {
  if [[ ! -f "$XML" ]]; then
    echo "Pas de $XML pour $USER_NAME"
    return 1
  fi
  python3 - "$XML" "${FAV_IDS[@]}" <<'PY'
import sys
import xml.etree.ElementTree as ET

xml_path = sys.argv[1]
favs = sys.argv[2:]
tree = ET.parse(xml_path)
root = tree.getroot()
added = 0
for plugins in root.findall("property[@name='plugins']"):
    for prop in plugins.findall("property"):
        name = prop.get("name", "")
        if not name.startswith("plugin-") or prop.get("value") != "whiskermenu":
            continue
        fav_prop = prop.find("property[@name='favorites']")
        if fav_prop is None:
            fav_prop = ET.SubElement(prop, "property", {"name": "favorites", "type": "array"})
        existing = {v.get("value") for v in fav_prop.findall("value")}
        for fav in favs:
            if fav in existing:
                continue
            ET.SubElement(fav_prop, "value", {"type": "string", "value": fav})
            print(f"xml favori: {fav} → {name}")
            added += 1
if added:
    tree.write(xml_path, encoding="UTF-8", xml_declaration=True)
print(f"xml: {added} ajout(s)")
PY
}

# Live session (xfconfd) when graphical session is up.
whisker_plugins="$(run_as "xfconf-query -c xfce4-panel -p /plugins -lv 2>/dev/null \
  | grep whiskermenu \
  | sed -n 's|.*/plugins/plugin-\\([0-9]*\\).*|\\1|p'")"

added=0
if [[ -n "$whisker_plugins" ]]; then
  for pid in $whisker_plugins; do
    path="/plugins/plugin-${pid}/favorites"
    current="$(run_as "xfconf-query -c xfce4-panel -p ${path} -lv 2>/dev/null || true")"
    for fav in "${FAV_IDS[@]}"; do
      if grep -qF "value=\"${fav}\"" <<< "$current"; then
        continue
      fi
      if run_as "xfconf-query -c xfce4-panel -p ${path} -n -t string -s '${fav}' -a 2>/dev/null"; then
        echo "xfconf favori: $fav (plugin-${pid})"
        added=$((added + 1))
      elif run_as "xfconf-query -c xfce4-panel -p ${path} -t string -s '${fav}' -a 2>/dev/null"; then
        echo "xfconf favori: $fav (plugin-${pid})"
        added=$((added + 1))
      fi
    done
  done
fi

if [[ ! -f "$XML" ]] || ! grep -q whiskermenu "$XML"; then
  echo "No whiskermenu for $USER_NAME"
  exit 0
fi

# Offline / RDP : patch XML (xfconfd absent depuis SSH).
patch_xml_favorites || true

if [[ "$added" -eq 0 ]]; then
  echo "Favoris PoolSync OK pour $USER_NAME (reconnecter XFCE si menu inchangé)"
fi
