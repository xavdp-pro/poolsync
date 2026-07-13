#!/usr/bin/env bash
# Ajoute le plugin Indicator XFCE si absent (requis pour l'icône AppIndicator).
set -euo pipefail

USER_NAME="${1:-zaza}"
if [[ "$(id -un)" == "$USER_NAME" ]]; then
  RUN_AS=()
else
  RUN_AS=(su - "$USER_NAME" -c)
fi

run_xfconf() {
  if [[ ${#RUN_AS[@]} -gt 0 ]]; then
    "${RUN_AS[@]}" "$1"
  else
    bash -lc "$1"
  fi
}

has_indicator() {
  run_xfconf "xfconf-query -c xfce4-panel -p /plugins -lv 2>/dev/null | grep -q ' value=\"indicator\"'"
}

if has_indicator; then
  echo "Plugin Indicator déjà présent pour $USER_NAME"
  exit 0
fi

echo "Ajout plugin Indicator XFCE pour $USER_NAME"
# Prochain id libre (max plugin-N + 1)
NEXT_ID="$(run_xfconf "xfconf-query -c xfce4-panel -p /plugins -lv 2>/dev/null | sed -n 's|.*/plugins/plugin-\\([0-9]\\+\\).*|\1|p' | sort -n | tail -1")"
NEXT_ID=$((NEXT_ID + 1))

run_xfconf "xfconf-query -c xfce4-panel -p /plugins/plugin-${NEXT_ID} -n -t string -s indicator"
# Réinjecter la liste complète (évite d'écraser plugin-ids avec -a seul)
EXISTING_IDS="$(run_xfconf "xfconf-query -c xfce4-panel -p /panels/panel-0/plugin-ids -lv 2>/dev/null | sed -n 's|/panels/panel-0/plugin-ids/\\([0-9]*\\) .* value=\"\\([0-9]*\\)\"|\\2|p' | tr '\\n' ' '")"
if [[ -z "$EXISTING_IDS" ]]; then
  EXISTING_IDS="1 2 3 4 14 6 9 13 11 5 12"
fi
if ! grep -qw "$NEXT_ID" <<< "$EXISTING_IDS"; then
  EXISTING_IDS="$EXISTING_IDS $NEXT_ID"
fi
run_xfconf "xfconf-query -c xfce4-panel -p /panels/panel-0/plugin-ids -r 2>/dev/null || true"
IDX=0
for ID in $EXISTING_IDS; do
  run_xfconf "xfconf-query -c xfce4-panel -p /panels/panel-0/plugin-ids/${IDX} -n -t int -s ${ID}"
  IDX=$((IDX + 1))
done
run_xfconf "xfconf-query -c xfce4-panel -p /plugins/plugin-${NEXT_ID}/known-indicators -t string -s libayatana-application.so -a 2>/dev/null || true"

echo "Plugin Indicator ajouté (plugin-${NEXT_ID}). Redémarrez le panneau ou reconnectez la session."
