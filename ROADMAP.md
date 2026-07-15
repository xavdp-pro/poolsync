# PoolSync — Roadmap

## À faire (prochaine session)

### Fenêtre native depuis le systray (config + logs)

**Contexte :** aujourd'hui le systray (`poolsync-agent/src/tray.rs`) affiche un menu contextuel + « Voir les logs… » qui ouvre `logs_viewer.rs` (journalctl, lecture seule). La config serveur / topologie n'est accessible que via le dashboard web (`web/src/pages/Config.jsx`).

**Objectif :** depuis l'icône systray sur **chaque** agent, ouvrir une **fenêtre GTK native** (comme les logs) avec **onglets** :

| Onglet | Contenu |
|--------|---------|
| **Configuration** | Équivalent local de la page web : topologie mosaïque (voisins gauche/droite/haut/bas), `kvm_enabled` par nœud, hub URL, token — via API hub (`/api/topology`) + édition `~/.config/poolsync/agent.toml` pour les champs locaux (`node`, `screen`, `neighbors`, `mode`, etc.) |
| **Logs** | Vue logs actuelle (`logs_viewer.rs`) **+** panneau d'édition des paramètres agent (TOML ou formulaire) avec bouton Enregistrer / redémarrage agent optionnel |

**Comportement souhaité :**

- Clic systray (ou entrée menu « Configuration… ») → fenêtre unique réutilisable (comme `OPEN_WINDOW` dans `logs_viewer.rs`)
- Onglets : `Configuration` | `Logs & paramètres`
- Réutiliser les patterns existants : GTK + `glib::MainContext::invoke`, API hub déjà exposée côté web (`web/src/api.js`)
- Pas de remplacement du dashboard web — complément desktop pour configurer sans ouvrir le navigateur

**Fichiers concernés (indicatif) :**

- `poolsync-agent/src/tray.rs` — entrée menu + ouverture fenêtre
- `poolsync-agent/src/logs_viewer.rs` — fusionner ou factoriser en `settings_window.rs` avec onglets
- Nouveau : `config_viewer.rs` (ou module `ui/`) — formulaire topologie + agent.toml
- `poolsync-core` — helpers lecture/écriture config si besoin

**Référence web à reproduire (simplifié desktop) :** `web/src/pages/Config.jsx`, `web/src/api.js` (`fetchTopology`, `saveTopology`)

---

## Idées plus tard

- Molette souris (`MouseWheel`) dans le grab KVM (`kvm_input.rs`)
- Déploiement agents acer/inspiron si pas à jour (`./deploy/install-agent.sh`)
