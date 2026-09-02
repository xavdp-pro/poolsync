lxc exec poolsync-test -- bash -c '
for d in desk-a desk-b; do podman exec neko-$d sh -c "wc -l < /tmp/poolsync-agent.log" > /tmp/lines-$d; done
sleep 180
echo "== repos 3 min (banc) :"
for d in desk-a desk-b; do
  n0=$(cat /tmp/lines-$d)
  podman exec neko-$d sh -c "tail -n +$((n0+1)) /tmp/poolsync-agent.log" > /tmp/new-$d.log 2>/dev/null
  printf "   %-7s %s copies locales, %s notifications, %s reçues, %s WARN\n" $d "$(grep -c "clipboard local:" /tmp/new-$d.log)" "$(grep -c "notification envoyée" /tmp/new-$d.log)" "$(grep -c "synced via" /tmp/new-$d.log)" "$(grep -c " WARN " /tmp/new-$d.log)"
done
echo "   hub : $(curl -s "http://127.0.0.1:9470/api/status?token=$(sed -n "s/^POOLSYNC_TOKEN=//p" /srv/poolsync/hub.env)" | python3 -c "import sys,json; d=json.load(sys.stdin); print(\", \".join(n[\"name\"]+(\" en ligne\" if n[\"online\"] else \" HORS LIGNE\") for n in d[\"nodes\"]))")"
' </dev/null
