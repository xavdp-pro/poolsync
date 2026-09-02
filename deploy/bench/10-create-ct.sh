set -euo pipefail
CT=poolsync-test
if ! lxc info $CT >/dev/null 2>&1 </dev/null; then
  lxc launch images:debian/13 $CT -p podman-univ </dev/null
  sleep 8
fi
lxc exec $CT -- bash -c 'until ping -c1 -W2 deb.debian.org >/dev/null 2>&1; do sleep 1; done' </dev/null
lxc exec $CT -- bash -c 'export DEBIAN_FRONTEND=noninteractive; apt-get update -qq && apt-get install -y -qq podman fuse-overlayfs slirp4netns uidmap socat curl git python3 python3-yaml jq rsync openssh-server xdotool xclip >/dev/null 2>&1; podman --version; echo "ip: $(hostname -I)"' </dev/null
lxc exec $CT -- bash -c 'sysctl -w net.ipv4.ip_forward=1 >/dev/null; mkdir -p /srv/neko' </dev/null
echo "CT prêt : $(lxc list $CT -c s4 --format csv </dev/null)"
