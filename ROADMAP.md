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

## v2 — communications sécurisées ✅

Décidé le 29/08/2026, à la suite de la fuite du token du pool sur le dépôt
public (rotation effectuée, cf. `docs/tempete-presse-papiers-2026-08-29.md`).

Avant la v2, tout circulait **en clair** entre les nœuds et le token partagé
était placé dans les URL. Les risques qui ont motivé cette migration étaient :

- un nœud joignable hors VPN expose tout le contenu copié à qui écoute ;
- le token, une fois vu dans un log ou une capture, donne un accès complet ;
- l'authentification est un secret **partagé** : impossible de révoquer une
  seule machine sans changer le token de tout le pool (ce qu'on vient de faire,
  et qui demande de toucher au hub plus aux quatre agents).

### Fait (branche `clipboard-total-order`)

- **Gestionnaire de presse-papiers X11** (`clipboard_manager.rs`) : PoolSync sait
  tenir le rôle `CLIPBOARD_MANAGER` et recueillir la sélection d'une application
  qui se ferme. Il s'abstient quand un autre gestionnaire est déjà en place —
  c'est le cas sur les quatre machines du pool, la fonction y est donc inerte.
  Le filet qui agit réellement aujourd'hui est la reprise par sondage
  (`reclaim_orphaned_selection`) : le contenu survit à la fermeture de
  l'application qui l'avait copié.
- **API et WebSocket sans token dans l'URL** : toutes les routes privées et les
  handshakes hub/peer utilisent `Authorization: Bearer`.
- **Identité par nœud** : fichier JSON rechargeable à chaque connexion, ancien
  jeton accepté pendant rotation, révocation indépendante, identité liée au
  premier `Hello`.
- **TLS natif** : certificat/clef côté hub et listeners peer WSS; validation par
  le trust store côté client. Un générateur crée la CA et les secrets hors dépôt.
- **Presse-papiers E2E** : XChaCha20-Poly1305, avec refus des messages en clair
  après activation et option hub `--require-e2e`. Le hub ne voit plus les blobs.

### Wayland ✅ pour le clipboard et l'injection

Toutes les machines sont en X11 aujourd'hui, mais Debian 13 et Ubuntu poussent
Wayland : sans une couche d'abstraction « bureau », la v2 y serait aveugle.
Le presse-papiers natif utilise désormais `wl-clipboard` (texte, HTML, PNG/JPEG)
et n'exécute aucun probe X11. L'injection KVM receive-only passe par
`ydotool`/`uinput`. La capture globale Wayland exige une session de portail
RemoteDesktop/libei consentie par l'utilisateur : elle n'est volontairement pas
contournée; les nœuds Wayland doivent utiliser `kvm_capture = false`.

---

## Later ideas

- Mouse wheel (`MouseWheel`) in KVM grab (`kvm_input.rs`)
- Packaging / install script polish for multi-host deploy
