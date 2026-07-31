# Raccourci clavier PoolSync — activer / désactiver localement

## Raccourci retenu : **Ctrl+Alt+Shift+P**

| Combinaison | Verdict |
|-------------|---------|
| **Ctrl+Alt+Shift+P** | **Recommandé** — rare, mémorable (P = PoolSync), fonctionne Linux / macOS / Windows |
| Ctrl+Tab | À éviter — changement d’onglet (navigateur, IDE, terminal) |
| Ctrl+Échap | À éviter — sous Windows ouvre le menu Démarrer |
| Ctrl+Alt+P | Acceptable mais plus de risques de conflit (autres apps) |

Sur **Mac** : `Ctrl+Option+Shift+P` (Option = Alt).

## Comportement

- Bascule **KVM + presse-papiers** sur **la machine où tu appuies** sur le raccourci.
- Les autres nœuds du pool (Asus, Acer, etc.) **ne sont pas affectés**.
- Une **notification** confirme l’état :
  - *PoolSync activé* — KVM + clipboard actifs sur ce nœud
  - *PoolSync désactivé* — suspendu localement ; réappuyer sur le raccourci pour réactiver
- En désactivation, le curseur X11 est réaffiché (secours si KVM bloquait la souris).

## Prérequis

- Agent `poolsync-agent` en cours d’exécution (service user systemd).
- Session graphique X11 (XFCE) avec `notify-send` / `libnotify-bin`.
- Sous **Wayland pur**, le raccourci global peut être indisponible (limitation OS).

## Déploiement

Le raccourci est intégré au binaire `poolsync-agent` (module `hotkey.rs`, crate `global-hotkey`).

Machines mises à jour via install du binaire release + restart :

```bash
systemctl --user restart poolsync-agent
```

Vérifier dans les logs :

```bash
journalctl --user -u poolsync-agent | grep -i raccourci
# → raccourci Ctrl+Alt+Shift+P enregistré (toggle PoolSync local)
```

## Implémentation

- `poolsync-agent/src/hotkey.rs` — écoute globale
- `poolsync-agent/src/state.rs` — `toggle_local_poolsync()` / `local_poolsync_active`
- `poolsync-agent/src/notify_util.rs` — notification via `notify-send`
