lxc exec poolsync-test -- bash -c '
export DEBIAN_FRONTEND=noninteractive; apt-get install -y -qq gettext-base >/dev/null 2>&1 && echo "  envsubst: ok"
cd /srv/neko
for d in desk-a desk-b; do
  echo "== start $d"
  ./bin/neko-desk start $d 2>&1 | grep -vE "^\s*$|Pulling|Copying|Writing|Storing" | tail -8
done
sleep 8
echo "== conteneurs :"; podman ps -a --format "  {{.Names}} | {{.Status}}" | head
' </dev/null
