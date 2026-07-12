# now3pool

Pool **presse-papiers + clavier/souris** pour machines NOW3 — remplacement progressif de Barrier.

- **Hub** : coordinateur central (bs1, Asus, VPS ou container — au choix)
- **Agent** : un daemon par machine du pool (XFCE + clipman)
- **Transport** : WebSocket via VPN WireGuard (`wg-gbs` ou `wg-bs1`)
- **Maître dynamique** : la machine où vous tapez/bougez la souris devient maître

## Workspace Rust

| Crate | Rôle |
|-------|------|
| `now3pool-core` | Protocole JSON, config TOML |
| `now3pool-hub` | Serveur WebSocket |
| `now3pool-agent` | Client X11 (xclip, xdotool) |

## Build

```bash
cargo build --release
```

Binaires : `target/release/now3pool-hub`, `target/release/now3pool-agent`

## Hub (flexible)

```bash
now3pool-hub --listen 0.0.0.0:9470 --token VOTRE_TOKEN
```

Peut tourner sur **bs1**, **Asus**, ou n'importe quel nœud joignable par les agents via VPN.

## Agent

Fichier `/etc/now3pool/agent.toml` :

```toml
node = "inspiron"
hub_url = "ws://10.24.42.1:9470/ws"
token = "VOTRE_TOKEN"
mode = "full"   # ou "clipboard_only"

[screen]
width = 1920
height = 1080

[[neighbors]]
direction = "left"
node = "acer"
```

## Déploiement NOW3 (phase pilote)

- **Hub** : `bs1` (Docker)
- **Agents** : `acer`, `inspiron` (Barrier reste actif sur les 3 portables)
- **Asus** : agent plus tard

## Licence

MIT — usage interne NOW3 / xavdp-pro
