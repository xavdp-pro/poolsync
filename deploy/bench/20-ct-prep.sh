set -euo pipefail
lxc exec poolsync-test -- bash -c '
export DEBIAN_FRONTEND=noninteractive
apt-get install -y -qq podman-compose >/dev/null 2>&1 && echo "  podman-compose: $(podman-compose --version 2>/dev/null | tail -1)"
cat > /srv/neko/node.yaml <<Y
# Banc de test PoolSync — conteneur LXD poolsync-test sur gbs-test
node_id: poolsync-test
proxmox_host: gbs-test
vpn_ip: 10.87.78.36
lan_ip: 10.213.199.137
http_base: 9081
ssh_base: 3222
udp_base: 52000
udp_block: 30
Y
mkdir -p /srv/neko/run /srv/neko/instances
[ -f /srv/neko/registry.json ] || echo "{\"instances\":{}}" > /srv/neko/registry.json
echo "  node.yaml écrit ; clés autorisées : $(wc -l < /srv/neko/keys/authorized_keys) ligne(s)"
echo -n "  image neko : "; podman image exists ghcr.io/m1k1o/neko/xfce:latest && echo "présente" || (echo "pull en cours"; tail -c 120 /tmp/neko-pull.log)
' </dev/null
