lxc exec poolsync-test -- bash -c '
cd /srv/neko
echo "  clés autorisées : $(cut -d" " -f3 keys/authorized_keys | tr "\n" " ")"
for d in desk-a desk-b; do
  if [ -d instances/$d ]; then echo "  $d existe déjà"; continue; fi
  echo "== création $d"
  ./bin/neko-desk create $d --user zaza --lang fr --profile minimal --screen 1600x900@30 2>&1 | grep -vE "^\s*$" | tail -6
done
echo "== conteneurs :"; podman ps --format "  {{.Names}} {{.Status}}"
' </dev/null
