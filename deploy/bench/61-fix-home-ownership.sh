lxc exec poolsync-test -- bash -c '
for d in desk-a desk-b; do
  podman exec neko-$d sh -c "chown zaza:zaza /home/zaza && chmod 755 /home/zaza && stat -c \"   $d: %U %a %n\" /home/zaza"
done
echo "== test ssh depuis le CT :"; ssh -o BatchMode=yes -o StrictHostKeyChecking=no -o ConnectTimeout=5 -p 3222 zaza@127.0.0.1 "echo \"   ok: \$(hostname) DISPLAY=\$DISPLAY\"" 2>&1 | tail -1
' </dev/null
