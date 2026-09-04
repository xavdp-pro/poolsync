# PoolSync — Roadmap

## Done

### Native window from systray (config + logs) ✅

Implemented in `poolsync-agent/src/config_window.rs`. The systray entry
**“Configuration du pool…”** opens a reusable GTK window (same `OPEN_WINDOW`
singleton pattern as `logs_viewer.rs`) with two tabs:

| Tab | Content |
|-----|---------|
| **Configuration** | Pool topology editor: per-node `kvm_enabled`, screen size, and left/right/up/down neighbours, fetched from and saved back to the hub via `GET`/`POST /api/topology` (using the agent's token). |
| **Logs** | Read-only `journalctl` view, reusing `logs_viewer::fetch_journal_logs`. |

Native desktop equivalent of `web/src/pages/Config.jsx`, no browser required.

**Done (v1.2+):** drag mosaic in GTK config window + auto neighbor inference from
screen positions (Barrier-style). Web config page has the same mosaic UX.

**Still open:** editing `~/.config/poolsync/agent.toml` local fields is available
in the Agent local tab; optional snap-to-edge polish.

---

## v2 — chiffrer les communications entre machines

Décidé le 29/08/2026, à la suite de la fuite du token du pool sur le dépôt
public (rotation effectuée, cf. `docs/tempete-presse-papiers-2026-08-29.md`).

Aujourd'hui tout circule **en clair** entre les nœuds : le presse-papiers
(texte *et* images) et les événements clavier/souris du KVM voyagent en
WebSocket non chiffré, et le token d'authentification est passé en **paramètre
d'URL** (`?token=…`), donc écrit tel quel dans les journaux d'accès du hub.

La confidentialité repose donc entièrement sur le fait que le pool tourne
au-dessus de WireGuard. C'est acceptable tant que c'est vrai, mais :

- un nœud joignable hors VPN expose tout le contenu copié à qui écoute ;
- le token, une fois vu dans un log ou une capture, donne un accès complet ;
- l'authentification est un secret **partagé** : impossible de révoquer une
  seule machine sans changer le token de tout le pool (ce qu'on vient de faire,
  et qui demande de toucher au hub plus aux quatre agents).

### Fait depuis (branche `clipboard-total-order`)

- **Gestionnaire de presse-papiers X11** (`clipboard_manager.rs`) : PoolSync sait
  tenir le rôle `CLIPBOARD_MANAGER` et recueillir la sélection d'une application
  qui se ferme. Il s'abstient quand un autre gestionnaire est déjà en place —
  c'est le cas sur les quatre machines du pool, la fonction y est donc inerte.
  Le filet qui agit réellement aujourd'hui est la reprise par sondage
  (`reclaim_orphaned_selection`) : le contenu survit à la fermeture de
  l'application qui l'avait copié.

Reste donc pour la v2 : le chiffrement et Wayland.

Pistes à trancher au moment de l'implémentation :

- **TLS sur le lien** (`wss://`) pour le hub comme pour le maillage direct,
  avec des certificats propres au pool. Simple, éprouvé, mais ne protège pas
  d'un hub compromis : celui-ci voit tout en clair.
- **Chiffrement de bout en bout de la charge utile** — le hub ne relaie que
  des blobs opaques et ne peut plus rien lire, ce qui vaut aussi pour son
  historique presse-papiers. Demande une gestion de clés entre nœuds.
- **Identité par nœud** (une clé par machine plutôt qu'un secret partagé),
  pour pouvoir révoquer une machine seule.
- Sortir le token de l'URL dans tous les cas, vers un en-tête ou une poignée
  de main applicative.

### Wayland

Toutes les machines sont en X11 aujourd'hui, mais Debian 13 et Ubuntu poussent
Wayland : sans une couche d'abstraction « bureau », la v2 y serait aveugle.
Le presse-papiers passerait par `ext-data-control` (ou `wlr-data-control`), et
le KVM par le portail RemoteDesktop / `libei` — deux protocoles sans rapport
avec les sélections X11, donc un second backend complet, pas une adaptation.

Le chiffrement de bout en bout est le seul qui rende la fuite d'un secret
non catastrophique ; c'est la direction à privilégier si le coût le permet.

---

## Later ideas

- Mouse wheel (`MouseWheel`) in KVM grab (`kvm_input.rs`)
- Packaging / install script polish for multi-host deploy
