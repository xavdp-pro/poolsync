set -euo pipefail
CT_IP=10.213.199.137; VPN_IP=10.87.78.36
sysctl -w net.ipv4.ip_forward=1 >/dev/null
add() { iptables -t nat -C PREROUTING "$@" 2>/dev/null || iptables -t nat -A PREROUTING "$@"; }
fwd() { iptables -C FORWARD "$@" 2>/dev/null || iptables -I FORWARD "$@"; }
add -d $VPN_IP -p tcp --dport 9081:9083 -j DNAT --to-destination $CT_IP
add -d $VPN_IP -p tcp --dport 3222:3224 -j DNAT --to-destination $CT_IP
add -d $VPN_IP -p tcp --dport 9300:9302 -j DNAT --to-destination $CT_IP
add -d $VPN_IP -p udp --dport 52000:52089 -j DNAT --to-destination $CT_IP
fwd -d $CT_IP -p tcp -m multiport --dports 9081:9083,3222:3224,9300:9302 -j ACCEPT
fwd -d $CT_IP -p udp --dport 52000:52089 -j ACCEPT
# le trafic retour vers le VPN doit être masqué (source = wg, pas lxdbr0)
iptables -t nat -C POSTROUTING -d $CT_IP -j MASQUERADE 2>/dev/null || iptables -t nat -A POSTROUTING -d $CT_IP -j MASQUERADE
echo "  règles DNAT/FORWARD posées sur gbs-test ($VPN_IP -> $CT_IP)"
iptables -t nat -S PREROUTING | grep -c "$CT_IP" | sed 's/^/  règles PREROUTING: /'
