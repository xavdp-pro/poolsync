# Banc de test PoolSync — deux (puis trois) bureaux XFCE dans un conteneur LXD

Objectif : reproduire le pool sur des bureaux jetables, piloter clavier/souris
au `xdotool`, et vérifier le copier-coller texte **et** image de bout en bout,
sans toucher aux vraies machines.

## Où

- Hôte : **gbs-test** (VPS Debian 13, LXD 5.0, Podman 5.4), IP WireGuard `10.87.78.36`.
- Conteneur LXD **`poolsync-test`** (Debian 13, profil `podman-univ` : nesting + privilégié),
  IP `10.213.199.137` sur `lxdbr0`.
- Dans le conteneur : Podman + l'outillage `vision-neko` dans `/srv/neko`,
  les binaires PoolSync dans `/srv/poolsync`, le hub de test en service systemd.
- Bureaux : `neko-desk-a`, `neko-desk-b` (image `ghcr.io/m1k1o/neko/xfce:latest`,
  Debian 13, XFCE 4, glibc 2.41, `xdotool` inclus), reliés par le réseau Podman
  `poolsync-net` (10.89.2.0/24). `desk-c` = Xubuntu 24.04 (recette `xubuntu/`).
- Snapshot LXD `base-2desks` : état propre avec deux bureaux et agents.

**Le banc est isolé du pool de prod** : hub et token dédiés (`/srv/poolsync/hub.env`),
aucun voisin vers acer/asus/gbs-p2/gbs-p3.

## Accès (depuis une machine du VPN)

| Bureau | Web Neko (WebRTC)        | ssh                            |
|--------|--------------------------|--------------------------------|
| desk-a | http://10.87.78.36:9081  | `ssh -p 3222 zaza@10.87.78.36` |
| desk-b | http://10.87.78.36:9082  | `ssh -p 3223 zaza@10.87.78.36` |
| desk-c (Xubuntu) | http://10.87.78.36:9083/vnc.html (noVNC, sans mot de passe, VPN seulement) | `ssh -p 3224 zaza@10.87.78.36` |

Mots de passe Neko : `neko-desk status desk-a` dans le conteneur. Rien n'est
exposé sur l'IP publique : le DNAT (`30-vpn-dnat.sh`) n'écoute que sur l'IP WireGuard.

Dans un bureau Neko, l'affichage est `DISPLAY=:99.0`, `XAUTHORITY=/data/xauthority` ;
sur desk-c (Xubuntu) `DISPLAY=:99` sans XAUTHORITY.

## Piloter un bureau

Depuis le conteneur (`lxc exec poolsync-test -- bash`) :

```bash
X="-u zaza -e HOME=/home/zaza -e DISPLAY=:99.0 -e XAUTHORITY=/data/xauthority"
podman exec $X neko-desk-a xdotool search --onlyvisible --name BANC-A windowactivate --sync
podman exec $X neko-desk-a xdotool type --delay 30 "texte"; podman exec $X neko-desk-a xdotool key Return
podman exec $X neko-desk-b xclip -selection clipboard -o
podman exec $X neko-desk-a xfce4-screenshooter -f -s /tmp/capture.png
```

Règle apprise : **activer et donner le focus à la fenêtre avant de taper**
(`windowactivate --sync` puis `windowfocus --sync`), sinon les premiers
caractères partent dans le vide.

## Scripts (dans l'ordre)

Ils s'exécutent depuis asus par `ssh gbs-test 'bash -s' < script` — jamais de
quoting imbriqué. Les commandes `lxc` doivent avoir `</dev/null`, sinon elles
avalent le reste du script sur stdin comme du YAML.

| Script | Rôle |
|--------|------|
| `10-create-ct.sh` | crée le conteneur LXD et installe podman, socat, xdotool, xclip |
| `20-ct-prep.sh` | podman-compose, `node.yaml` du banc, registre Neko |
| `30-vpn-dnat.sh` | DNAT IP WireGuard → conteneur (web, ssh, ctl, UDP WebRTC) |
| `40-create-desks.sh`, `41-start-desks.sh` | crée et démarre desk-a / desk-b |
| `50-hub-and-desk-tools.sh` | hub de test (systemd), ports ctl, xclip/gi dans les bureaux |
| `51-network-and-agent-configs.sh` | réseau `poolsync-net`, `agent.toml` et binaire par bureau |
| `52-launch-agents.sh` | lance les agents, vérifie le maillage et le hub |
| `60-persist-and-snapshot.sh` | unités systemd (démarrage + relance toutes les 2 min), snapshot |
| `61-fix-home-ownership.sh` | `/home/zaza` doit appartenir à zaza, sinon sshd refuse les clés |
| `90-test-text-and-image.sh` | test de bout en bout : frappe réelle, Ctrl+Shift+V, capture d'écran |
| `91-diag-typing.sh` | diagnostic focus/frappe |
| `70-desk-c-xubuntu.sh` | lance desk-c (image `bench-xubuntu`), relais 9083/3224, agent, voisinage b↔c |
| `71-desk-c-xfce-restart.sh` | (historique) dbus-x11 manquait dans l'image : XFCE ne démarrait pas |
| `92-test-multihop.sh`, `93-test-paste-xubuntu.sh`, `94-soak-idle.sh` | multi-sauts a→b→c, collage réel sur Xubuntu, repos 3 min |
| `xubuntu/` | image Xubuntu 24.04 (Xorg dummy + XFCE + dbus-x11 + noVNC + ssh) pour desk-c |

## Topologie

```
desk-a (Debian 13) ── desk-b (Debian 13) ── desk-c (Xubuntu 24.04)
        \_______________ hub (conteneur, 10.89.2.1:9470) _______________/
```

## Résultats de référence (02/09/2026)

- texte tapé sur desk-a, collé par Ctrl+Shift+V sur desk-b dans un fichier : identique ;
- capture d'écran réelle (`xfce4-screenshooter -f -c`) sur desk-a : `image/png` de
  60 064 octets, empreinte identique sur desk-b, 8 cibles annoncées (PNG + BMP + texte).
- multi-sauts : texte desk-a → desk-b → desk-c identique ; capture réelle sur la Xubuntu
  (46 008 octets) identique sur desk-b et desk-a ;
- collage réel (Ctrl+Shift+V) dans un `xfce4-terminal` de la Xubuntu : identique ;
- repos 3 min sur desk-a/desk-b : 0 copie locale, 0 notification, 0 WARN.
