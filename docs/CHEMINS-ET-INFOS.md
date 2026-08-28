# PoolSync — chemins et infos

> Document de référence unique : chemins, topologie, ports, commandes et déploiement.

---

## Identité

| Élément | Valeur |
|---------|--------|
| **Projet** | PoolSync — presse-papiers partagé + KVM (alternative à Barrier) |
| **Repo GitHub** | https://github.com/xavdp-pro/poolsync |
| **Branche active** | `v1.2` |
| **Langage** | Rust (workspace Cargo) |
| **Principe** | Maître dynamique — la machine utilisée devient le maître clavier/souris |

---

## Code source (local)

| Élément | Chemin |
|---------|--------|
| **Racine projet** | `/home/zaza/Bureau/NOW3/mds/poolsync-v1/` |
| **Crate core** (protocole, config TOML) | `mds/poolsync-v1/poolsync-core/` |
| **Crate hub** (coordinateur WebSocket) | `mds/poolsync-v1/poolsync-hub/` |
| **Crate agent** (clipboard, KVM, systray) | `mds/poolsync-v1/poolsync-agent/` |
| **Scripts deploy** | `mds/poolsync-v1/deploy/` |
| **Configs agents (templates)** | `mds/poolsync-v1/deploy/config/` |
| **Docs** | `mds/poolsync-v1/docs/` |
| **Binaires build** | `mds/poolsync-v1/target/release/poolsync-{hub,agent}` |

### Workspace Rust

| Crate | Rôle |
|-------|------|
| `poolsync-core` | Protocole JSON, config TOML, topologie |
| `poolsync-hub` | Serveur WebSocket + dashboard web |
| `poolsync-agent` | Client X11 (clipboard, KVM, systray) |

---

## Hub (serveur central — gbs-p3)

| Élément | Chemin / valeur |
|---------|-----------------|
| **Serveur** | `gbs-p3` (VPN `10.87.78.22`) |
| **Répertoire** | `/opt/poolsync/` |
| **Binaire** | `/opt/poolsync/poolsync-hub` |
| **Token** | `/opt/poolsync/poolsync.env` |
| **Dashboard web** | `/opt/poolsync/web/` |
| **Service systemd** | `/etc/systemd/system/poolsync-hub.service` |
| **Port** | **9470** |
| **URL agents** | `ws://10.87.78.22:9470/ws` |
| **Script deploy** | `mds/poolsync-v1/deploy/install-hub-gbs-p3.sh` |

Ancien hub `bs1` (`10.24.42.1`) : hors service, ne plus l’utiliser.

```bash
# Déployer le hub
cd /home/zaza/Bureau/NOW3/mds/poolsync-v1
POOLSYNC_TOKEN=xxx ./deploy/install-hub-bs1.sh

# Vérifier
ssh bs1 'systemctl status poolsync-hub'
ssh bs1 'ss -tlnp | grep 9470'
```

### Lancement manuel (dev)

```bash
poolsync-hub --listen 0.0.0.0:9470 --token YOUR_TOKEN
```

---

## Agent (par machine — asus, acer, inspiron)

### Chemins locaux (user `zaza`)

| Élément | Chemin |
|---------|--------|
| **Binaire** | `~/.local/bin/poolsync-agent` |
| **Lanceur** | `~/.local/bin/poolsync-agent-launch.sh` |
| **Config** | `~/.config/poolsync/agent.toml` |
| **Icône systray** | `~/.local/share/poolsync/poolsync-tray.png` |
| **Cache clipboard** | `~/.cache/poolsync/clipboard/` |
| **Service systemd user** | `~/.config/systemd/user/poolsync-agent.service` |
| **Autostart XFCE** | `~/.config/autostart/poolsync-agent.desktop` |
| **Watchdog** | `~/.config/systemd/user/poolsync-watchdog.{service,timer}` |
| **Applications desktop** | `~/.local/share/applications/com.xavdp.poolsync*.desktop` |

### Outils CLI installés

| Commande | Chemin | Rôle |
|----------|--------|------|
| `poolsync-ctl` | `~/.local/bin/poolsync-ctl` | start / stop / restart / toggle |
| `poolsync-logs` | `~/.local/bin/poolsync-logs` | journal agent |
| `poolsync-test` | `~/.local/bin/poolsync-test` | suite anti-régression |
| `poolsync-watchdog.sh` | `~/.local/bin/poolsync-watchdog.sh` | surveillance agent |
| `read-image-clipboard.py` | `~/.local/bin/read-image-clipboard.py` | secours images GTK |
| `write-image-clipboard.py` | `~/.local/bin/write-image-clipboard.py` | écriture images clipboard |

### Script install local

```bash
cd /home/zaza/Bureau/NOW3/mds/poolsync-v1
POOLSYNC_TOKEN=xxx ./deploy/install-agent-local.sh asus   # ou acer, inspiron
```

Les agents démarrent via **systemd user** + **autostart XFCE** après login graphique.

### gbs-p2 (session `zaza` uniquement — presse-papiers, pas de KVM)

| Élément | Valeur |
|---------|--------|
| **Nœud** | `gbs-p2` (`10.87.78.3`) |
| **Compte** | `zaza` seulement (pas `zaza2`, pas `root`) |
| **Mode** | `clipboard_only` / `kvm_enabled = false` |
| **Display** | auto : session RDP **vivante** de `zaza` (`xrdp-chansrv` non zombie), via `poolsync-pick-session.sh` |
| **Hub** | `ws://10.87.78.22:9470/ws` |
| **Peer clipboard** | `ws://10.87.78.5:9472/ws` (asus, VPN) |
| **Asus → gbs-p2** | asus `hub_clipboard = true` + neighbor `gbs-p2` (`ws://10.87.78.3:9472/ws`) so copies reach the RDP session |
| **Script deploy** | `deploy/install-agent-gbs-p2.sh` |
| **Binaire** | conserver le build Debian 12 déjà sur p2 (glibc 2.36) — ne pas copier le binaire asus (glibc 2.39) |

```bash
cd /home/zaza/Bureau/NOW3/mds/poolsync-v1
POOLSYNC_TOKEN=xxx ./deploy/install-agent-gbs-p2.sh
```

### gbs-p3 (session `zaza` uniquement — presse-papiers, pas de KVM)

| Élément | Valeur |
|---------|--------|
| **Nœud** | `gbs-p3` (`10.87.78.22`) — hub PoolSync sur la même machine |
| **Compte** | `zaza` seulement (pas `zaza2`, pas `root`) |
| **Mode** | `clipboard_only` / `kvm_enabled = false` |
| **Display** | auto : session RDP **vivante** de `zaza` (`xrdp-chansrv` non zombie), via `poolsync-pick-session.sh` |
| **Hub** | `ws://10.87.78.22:9470/ws` (local) |
| **Peer clipboard** | `ws://10.87.78.5:9472/ws` (asus, VPN) |
| **Script deploy** | `deploy/install-agent-gbs-p3.sh` |
| **Binaire** | conserver le build Debian 12 déjà sur p3 (glibc 2.36) |

```bash
cd /home/zaza/Bureau/NOW3/mds/poolsync-v1
POOLSYNC_TOKEN=xxx ./deploy/install-agent-gbs-p3.sh
```

---

## Topologie du pool

```
[asus] ──right──> [acer] ──right──> [inspiron]
```

| Nœud | VPN IP | LAN IP (peer) | Config template |
|------|--------|---------------|-----------------|
| **asus** | `10.24.42.6` | `192.168.1.17` | `deploy/config/agent.asus.toml` |
| **acer** | `10.24.42.4` | `192.168.1.183` | `deploy/config/agent.acer.toml` |
| **inspiron** | `10.24.42.5` | `192.168.1.238` | `deploy/config/agent.inspiron.toml` |

### Voisinage configuré

| Nœud | Direction | Voisin | peer_url (LAN) | peer_url_vpn |
|------|-----------|--------|----------------|--------------|
| asus | right | acer | `ws://192.168.1.183:9472/ws` | `ws://10.24.42.4:9472/ws` |
| acer | left | asus | `ws://192.168.1.17:9472/ws` | `ws://10.24.42.6:9472/ws` |
| acer | right | inspiron | `ws://192.168.1.238:9472/ws` | `ws://10.24.42.5:9472/ws` |
| inspiron | left | acer | `ws://192.168.1.183:9472/ws` | `ws://10.24.42.4:9472/ws` |

### Ports

| Service | Port |
|---------|------|
| Hub WebSocket | **9470** |
| Peer mesh (clipboard direct P2P) | **9472** |

### Options actuelles (v1.2)

| Option | Valeur | Effet |
|--------|--------|-------|
| `peer_direct_clipboard` | `true` | Lien direct entre voisins (mesh) |
| `hub_clipboard` | `false` | Clipboard ne passe plus par le hub |
| `mode` | `full` | Clipboard + KVM |
| `kvm_enabled` | `true` | Basculement souris/clavier actif |
| `clipboard_poll_ms` | `100` | Intervalle de poll clipboard |

---

## Commandes utiles

```bash
# Statut agent
systemctl --user status poolsync-agent

# Redémarrer
poolsync-ctl restart

# Arrêter / démarrer
poolsync-ctl stop
poolsync-ctl start

# Logs
poolsync-logs           # 100 dernières lignes
poolsync-logs -f        # suivi live
poolsync-logs -n 500    # 500 lignes

# Toggle local (équivalent Ctrl+Alt+Shift+P)
poolsync-ctl toggle

# Vider l'historique clipboard
poolsync-ctl clear-history

# Tests anti-régression
poolsync-test              # complet (rust + intégration asus↔acer)
poolsync-test --local      # local seulement
poolsync-test --quick      # sans E2E réseau inter-nœuds

# Vérifier sur une machine distante
ssh acer 'systemctl --user status poolsync-agent'
ssh inspiron-gbs 'systemctl --user status poolsync-agent'
```

### poolsync-ctl — commandes

| Commande | Action |
|----------|--------|
| `restart` | Redémarre l'agent |
| `stop` | Arrête l'agent |
| `start` | Démarre l'agent |
| `status` | Vérifie si actif |
| `logs` | Affiche le journal |
| `toggle` | Active/désactive PoolSync localement |
| `clear-history` | Vide le cache historique clipboard |

---

## Keyboard shortcuts and systray

Canonical doc (English): **[keyboard-shortcuts.md](keyboard-shortcuts.md)**

| Action | Detail |
|--------|--------|
| **Pause shortcut** | `Ctrl+Alt+Shift+P` — toggle PoolSync on this machine only |
| **Master shortcut** | `Ctrl+Alt+Shift+M` — claim keyboard/mouse (KVM master) on this machine |
| **Center shortcut** | `Ctrl+Alt+Shift+C` — warp pointer to the center of the current monitor |
| **Locate shortcut** | `Ctrl+Alt+Shift+L` — ripple at the pointer + notification with the computer name |
| **Systray** | Clipboard ON/OFF, Become KVM master, locate cursor, restart, quit, configuration |
| **Notifications** | Copied / Received / ACTIVÉ / DÉSACTIVÉ (master-change toasts: systray debug checkbox, off by default) |
| **D-Bus app ID** | `com.xavdp.poolsync` |
| **Hotkey module** | `poolsync-agent/src/hotkey.rs` |

When PoolSync is paused locally, KVM and clipboard are off on this node only. Press P again to resume: that node **claims KVM master** so edge switching works without Ctrl+Alt+Shift+M.

---

## Build

```bash
cd /home/zaza/Bureau/NOW3/mds/poolsync-v1

cargo build --release                    # tout le workspace
cargo build --release -p poolsync-agent  # agent seul
cargo build --release -p poolsync-hub    # hub seul

# Tests unitaires
cargo test -p poolsync-core -p poolsync-agent
```

Binaires produits :

- `target/release/poolsync-hub`
- `target/release/poolsync-agent`

---

## Fichiers deploy importants

| Fichier | Rôle |
|---------|------|
| `deploy/install-agent-local.sh` | Install agent sur machine courante |
| `deploy/install-hub-bs1.sh` | Deploy hub sur bs1 |
| `deploy/poolsync-agent-launch.sh` | Lanceur avec env XFCE/X11 |
| `deploy/poolsync-ctl.sh` | Contrôle CLI |
| `deploy/poolsync-logs.sh` | Affichage logs |
| `deploy/poolsync-test.sh` | Tests anti-régression |
| `deploy/poolsync-watchdog.sh` | Surveillance agent |
| `deploy/poolsync-session-start.sh` | Démarrage session |
| `deploy/poolsync-enable-user.sh` | Activation user systemd |
| `deploy/read-image-clipboard.py` | Secours lecture images GTK |
| `deploy/write-image-clipboard.py` | Écriture images clipboard |
| `deploy/kvm-debug.sh` | Debug KVM / hub |
| `deploy/systemd/poolsync-hub.service` | Unit hub (system) |
| `deploy/systemd/poolsync-agent.service` | Unit agent (user) |
| `deploy/systemd/poolsync-watchdog.service` | Watchdog agent |
| `deploy/systemd/poolsync-watchdog.timer` | Timer watchdog |
| `deploy/autostart/poolsync-agent.desktop` | Autostart XFCE |
| `deploy/com.xavdp.poolsync.desktop` | Raccourci bureau |
| `deploy/com.xavdp.poolsync-restart.desktop` | Action redémarrer |
| `deploy/com.xavdp.poolsync-stop.desktop` | Action arrêter |
| `deploy/Dockerfile.hub` | Image Docker hub |
| `deploy/docker-compose.yml` | Compose hub |

---

## Config agent — exemple (`agent.toml`)

```toml
node = "asus"
hub_url = "ws://10.24.42.1:9470/ws"
token = "YOUR_TOKEN"
mode = "full"
kvm_enabled = true
clipboard_poll_ms = 100
peer_listen_port = 9472
peer_direct_clipboard = true
hub_clipboard = false

[screen]
width = 1344
height = 756

[[neighbors]]
direction = "right"
node = "acer"
peer_url = "ws://192.168.1.183:9472/ws"
peer_url_vpn = "ws://10.24.42.4:9472/ws"
```

Templates par nœud : `deploy/config/agent.{asus,acer,inspiron}.toml`

---

## Architecture

```
┌─────────┐     ws://9470      ┌──────────────┐     ws://9470      ┌───────────┐
│  asus   │◄──────────────────►│  hub (bs1)   │◄──────────────────►│ inspiron  │
│ agent   │                    │ 10.24.42.1   │                    │  agent    │
└────┬────┘                    └──────────────┘                    └─────┬─────┘
     │                                                                   │
     │              peer mesh ws://9472 (clipboard P2P)                  │
     └──────────────────────────► acer ◄────────────────────────────────┘
```

- **Hub** : coordinateur léger (topologie, focus KVM, présence)
- **Peer mesh** : clipboard direct entre voisins (LAN ou VPN)
- **Agent** : daemon X11 par machine (clipboard, KVM grab, systray)

---

## Sécurité

PoolSync est conçu pour un **réseau privé** (WireGuard VPN, LAN).

| Point | Détail |
|-------|--------|
| Transport | `ws://` non chiffré — confidentialité via VPN/LAN |
| Token | Authentifie, ne chiffre pas — passé en query param (`/ws?token=…`) |
| Autorisation | Pas de contrôle par nœud — tout client avec token valide rejoint le pool |
| Exposition | Ne pas exposer le port 9470 sur Internet sans TLS + contrôle d'accès |

---

## Documentation complémentaire

| Fichier | Contenu |
|---------|---------|
| `README.md` | Vue d'ensemble, build, sécurité |
| `PITCH.md` | Présentation produit, comparaison Barrier |
| `ROADMAP.md` | Feuille de route |
| `docs/keyboard-shortcuts.md` | Ctrl+Alt+Shift+P (pause) and M (KVM master) — English |
| `docs/fix-kvm-grab-souris-bloquee-2026-07-18.md` | Fix souris bloquée KVM |
| `docs/CHEMINS-ET-INFOS.md` | Ce document |

---

## Modules agent (code)

| Fichier | Rôle |
|---------|------|
| `poolsync-agent/src/main.rs` | Point d'entrée |
| `poolsync-agent/src/agent.rs` | Boucle principale, connexion hub |
| `poolsync-agent/src/state.rs` | État global, toggle local |
| `poolsync-agent/src/clipboard.rs` | Poll et envoi clipboard |
| `poolsync-agent/src/clipboard_incoming.rs` | Réception clipboard |
| `poolsync-agent/src/clipboard_history.rs` | Historique local |
| `poolsync-agent/src/peer_mesh.rs` | Mesh P2P entre voisins |
| `poolsync-agent/src/kvm.rs` | Logique KVM (bords, focus) |
| `poolsync-agent/src/kvm_x11.rs` | Grab X11 souris/clavier |
| `poolsync-agent/src/hotkey.rs` | Global hotkeys Ctrl+Alt+Shift+P / M / C / L |
| `poolsync-agent/src/tray.rs` | Menu systray XFCE |
| `poolsync-agent/src/config_window.rs` | Fenêtre configuration |
| `poolsync-agent/src/notify_util.rs` | Notifications notify-send |
| `poolsync-core/src/lib.rs` | Types, messages, config |
| `poolsync-core/src/topology.rs` | Géométrie mosaïque écrans |

---

## Dépannage rapide

| Symptôme | Action |
|----------|--------|
| Systray ne répond pas | `poolsync-ctl restart` |
| Souris bloquée après KVM | `poolsync-ctl restart` (voir `docs/fix-kvm-grab-souris-bloquee-2026-07-18.md`) |
| Notifications absentes | Vérifier `xfce4-notifyd` : `systemctl --user status xfce4-notifyd` |
| Hub inaccessible | `ssh bs1 'systemctl status poolsync-hub'` |
| Clipboard ne sync pas | `poolsync-test --quick` puis `poolsync-logs -f` |
| CPU élevé (Python) | Vérifier que `read-image-clipboard.py` n'est pas en boucle (cooldown 3s) |

---

*Dernière mise à jour : août 2026 — branche v1.2*
