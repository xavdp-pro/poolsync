# Correctif KVM — souris bloquée sur Asus (18 juillet 2026)

## Symptôme

Sur **Asus**, après un basculement KVM vers **Acer** (ou inversement), la souris et parfois le clavier restaient **figés** sur Asus. Un redémarrage de `poolsync-agent` était nécessaire pour retrouver le contrôle.

Sur **Acer**, le KVM semblait fonctionner correctement dans les mêmes conditions.

## Contexte matériel Asus

Layout X11 (`xrandr`) sur Asus au moment du diagnostic :

```
Bureau X11 total : 3263 × 1369

├── HDMI-1  : 1920×1080 @ (0, 0)        ← écran externe à gauche
└── eDP-1   : 1344×756  @ (1920, 614)   ← portable (moniteur primaire RandR, pool KVM)
```

La configuration PoolSync (`agent.toml`) ne décrit que l’eDP :

```toml
[screen]
width = 1344
height = 756
```

Le KVM PoolSync ne gère les **bords de switch** que sur le moniteur primaire du pool (eDP), pas sur l’HDMI. Les coordonnées `(3263, y)` visibles dans les logs correspondent au **bord droit** de l’eDP (`1920 + 1344 ≈ 3264`), là où le voisin `acer` est configuré (`direction = "right"`).

Ce layout dual-écran **aggrave** le ressenti utilisateur (souris qui « disparaît » vers l’eDP), mais **n’est pas la cause racine** du blocage total.

## Cause racine

Fichier concerné : `poolsync-agent/src/kvm.rs`

Quand une machine **pilote un nœud distant** (`focus != local` et `is_input_owner()`), PoolSync active un **grab X11** souris + clavier via `InputGrab::begin()` (style Barrier / Synergy).

Quand le focus **input** repasse à l’autre machine (`kvm_input_node != local`, donc `!is_input_owner()`), la boucle KVM faisait un `continue` **sans libérer le grab** :

```rust
// AVANT (bug)
if !state.is_input_owner() {
    let input_node = state.kvm_input_node();
    if input_node != local {
        last_phys = (px, py);
        thread::sleep(poll);
        continue;   // ← grab actif, souris bloquée !
    }
    // ...
}
```

Conséquences :

1. Le grab X11 restait actif sur Asus alors qu’Acer possédait l’input.
2. Le curseur pouvait rester masqué (`xfixes::hide_cursor`).
3. Les redémarrages rapides de l’agent (watchdog, reconnexion hub) pouvaient laisser un grab orphelin si l’agent s’arrêtait mid-grab.

## Correctif appliqué

```rust
// APRÈS (corrigé)
if input_node != local {
    input_grab = None;                      // Drop → ungrab_pointer + ungrab_keyboard
    set_cursor_visible_best_effort(true);   // réaffiche le curseur X11
    last_phys = (px, py);
    thread::sleep(poll);
    continue;
}
```

`input_grab = None` déclenche le `Drop` de `InputGrab`, qui appelle explicitement :

- `ungrab_pointer`
- `ungrab_keyboard`
- `unmap_window` / `destroy_window` de la fenêtre de grab

## Déploiement (18/07/2026)

| Machine | Action |
|---------|--------|
| **Asus** | patch `kvm.rs` → `cargo build --release -p poolsync-agent` → install `~/.local/bin/poolsync-agent` → `systemctl --user restart poolsync-agent` |
| **Acer** | binaire synchronisé depuis Asus (même hash MD5) → restart service |

Binaire déployé : `e1110b323056f52eb7391dfeff550342`

## Déblocage d’urgence (si récidive)

Sur la machine bloquée :

```bash
systemctl --user restart poolsync-agent
```

Si le curseur reste invisible :

```bash
DISPLAY=:0 xfixes -show-cursor root 2>/dev/null || true
pkill -x poolsync-agent
systemctl --user start poolsync-agent
```

## Procédure de test

1. Sur Asus, amener la souris au **bord droit de l’eDP** (pas HDMI) → switch vers Acer.
2. Utiliser Acer normalement.
3. Revenir sur Asus (bord gauche d’Acer ou mouvement physique sur Asus).
4. Vérifier que souris + clavier répondent sur **Asus entier** (HDMI + eDP).

## Piste d’évolution (non corrigée ici)

- Support explicite du **dual-écran** dans la topologie KVM (HDMI + eDP) avec offsets xrandr.
- Historique Cursor / prompts utilisateur : « j’arrive à aller sur l’autre portable mais pas sur les écrans HDMI de l’autre ».

Voir `poolsync-agent/src/kvm_x11.rs` (`kvm_display()`, `kvm_desktop()`) pour la détection RandR déjà en place.

---

## Correctif complémentaire (18/07 — rebond KVM + reclaim physique)

Le premier correctif (libération du grab) provoquait des **rebonds immédiats** Acer↔Asus : le curseur entrait sur le bord opposé et ressortait aussitôt.

### Changements supplémentaires

1. **`nudge_kvm_enter`** (`kvm_x11.rs`) — repousse le curseur à l'intérieur du pool après un `SwitchTo`.
2. **`block_entry_edge`** (`kvm.rs`) — bloque le bord d'entrée sur la machine distante pendant 700 ms.
3. **`try_physical_claim`** — permet à Asus (ou tout nœud) de **reprendre l'input** en bougeant la souris physiquement, même si un autre nœud possède le focus KVM.

### HDMI depuis Acer

Une fois sur Asus sans rebond, déplacer la souris **vers la gauche** traverse le bureau X11 complet (3263 px) jusqu'à l'écran HDMI (x≈0). Les bords KVM restent limités au moniteur pool (eDP), pas à l'HDMI.
