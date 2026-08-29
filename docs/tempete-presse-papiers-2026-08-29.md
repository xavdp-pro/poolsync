# Tempête presse-papiers — le pool diffusait ses propres sondes X11 (29 août 2026)

## Symptôme

Le presse-papiers changeait tout seul sur les quatre machines du pool, plusieurs fois par minute, avec un contenu que personne n'avait copié. Les notifications « PoolSync — Reçu » défilaient en continu et l'agent consommait **45 % de CPU** sur asus (1 h 48 min de CPU en 4 h de service).

Un échantillonnage du presse-papiers d'asus toutes les 4 secondes montre deux charges qui alternent :

```
11:17:28  TIMESTAMP|TARGETS|UTF8_STRING|STRING
11:17:32  451463243
11:17:36  TIMESTAMP|TARGETS|UTF8_STRING|STRING
11:17:44  451475196          ← le nombre augmente
```

- **36 octets** = la sortie de `xclip -selection clipboard -t TARGETS -o`
- **9 octets** = la sortie de `xclip -selection clipboard -t TIMESTAMP -o`, qui **change à chaque sonde**

C'est ce second point qui rendait la boucle inextinguible : ce n'était pas un vieux message qui rebouclait, mais du contenu neuf fabriqué en permanence.

## Localisation de l'émetteur

Les compteurs de journaux séparent nettement victime et coupable :

| Nœud | « Copié » (émission) | « synced via peer » (réception) |
|------|----------------------|--------------------------------|
| acer | 156 en 15 min | 0 |
| asus | 0 | 261 en 20 min |

acer émettait sans rien recevoir ; asus subissait sans rien émettre.

## Cause racine

Sur acer, un **propriétaire de sélection X11 à moitié mort** possédait `CLIPBOARD` : il annonçait des cibles texte dans `TARGETS` mais n'honorait plus aucune demande de conversion.

```
TEXT         rc=1  0 octet
STRING       rc=1  0 octet
UTF8_STRING  rc=1  0 octet
text/plain   rc=1  0 octet
TARGETS      rc=0  36 octets   ← seule à répondre
TIMESTAMP    rc=0  9 octets    ← seule à répondre
```

`Gtk.Clipboard.wait_for_text()` renvoyait `None` : **aucune application ne pouvait coller** sur acer, indépendamment de PoolSync.

Dans cet état, toutes les lectures de texte de l'agent échouaient et il ne restait que les deux sondes de métadonnées. L'agent prenait leur sortie pour le presse-papiers de l'utilisateur, la mettait en cache, l'affichait dans l'historique et la diffusait à tout le pool. Chaque nœud l'appliquait, la remettait dans sa propre sélection, et le cycle repartait avec une valeur d'horloge fraîche.

L'offre GTK de l'agent se dégrade de la même façon : elle sert correctement le texte pendant quelques secondes, **puis** garde la sélection sans plus répondre. Une vérification immédiate après écriture ne voyait donc rien.

## Correctif

Fichiers concernés : `poolsync-agent/src/clipboard.rs`, `clipboard_gtk.rs`, `clipboard_incoming.rs`.

Quatre garde-fous, du plus structurel au plus défensif.

### 1. Une lecture de contenu ne peut demander qu'une cible texte

```rust
async fn read_text_selection_bytes(selection: &str, target: &str, limit: Duration) -> Result<Vec<u8>> {
    if !X11_TEXT_TARGETS.iter().any(|t| t.eq_ignore_ascii_case(target)) {
        anyhow::bail!("refus de lire {target} comme du texte");
    }
    read_selection_bytes_timeout(selection, target, limit).await
}
```

`read_plain_text` et `stable_primary_text` passent par là. Aucun chemin, présent ou futur, ne peut plus renvoyer la sortie d'une sonde comme du texte.

### 2. On ne relit jamais notre propre offre GTK

`clipboard_gtk::owns_text_clipboard()`, symétrique de `owns_image_clipboard()` qui existait déjà pour les images et manquait pour le texte. Quand l'offre est la nôtre, on sait déjà ce qu'elle contient : inutile de la relire par `xclip`.

### 3. Vérifier que l'offre sert vraiment, sinon repli sur `xclip`

`ensure_text_is_actually_served()` revient à **250 ms, 1,5 s et 4 s** après l'écriture — l'offre se dégradant *après* coup, une seule vérification immédiate ne voyait rien. Si le texte n'est plus servi, on rend la sélection GTK et on repasse par un propriétaire `xclip` détaché, qui lui sert de façon fiable. C'est ce qui rétablissait le collage sur acer avant son redémarrage.

### 4. Filtres de contenu, à l'émission comme à la réception

- `is_target_list_dump()` — une liste d'atomes X11 (au moins deux atomes *connus*, pour ne pas confondre avec une liste de chemins copiée par l'utilisateur).
- `is_server_clock_echo()` — un entier de 6 à 12 chiffres à moins de **60 s** de l'horloge du serveur X.

Le filtre d'horloge a demandé deux itérations, les deux erreurs n'étant visibles qu'en production :

1. il comparait d'abord par **égalité stricte** au `TIMESTAMP` de la sélection — or les deux valeurs viennent de deux appels `xclip` successifs, entre lesquels l'horloge avance de quelques centaines de millisecondes. Il ne matchait jamais ;
2. il était ensuite **entièrement sauté quand la lecture de `TIMESTAMP` échouait**, c'est-à-dire exactement sur les sélections dégradées — le seul cas où il sert.

La référence est désormais l'horloge lue à l'instant si possible, **sinon extrapolée** depuis la dernière valeur observée (`LAST_SERVER_CLOCK` + `Instant::elapsed`). Un nombre copié par l'utilisateur n'est pas concerné : il faudrait qu'il tombe à moins d'une minute du compteur millisecondes du serveur X au moment précis de la lecture.

Les filtres s'appliquent **aussi à la réception** : un nœud corrigé ne peut pas être re-pollué par un nœud resté sur l'ancien binaire.

## Traçabilité ajoutée

Le diagnostic a demandé une heure de sondage manuel faute de traces. Trois lignes évitent de le refaire :

- `clipboard local: mime=… bytes=… preview=…` à chaque copie locale détectée — nomme immédiatement ce que l'agent croit avoir copié ;
- `clipboard read source=…` (debug) — nomme la branche de lecture qui a produit le payload ;
- `propriétaire de sélection cassé — cibles annoncées (…) mais aucun texte lisible` (avertissement, une fois par minute au plus) — nomme l'état pathologique.

C'est la première de ces lignes qui a tranché : `preview="455772428"`, exactement la valeur renvoyée par `xclip -t TIMESTAMP -o` au même instant.

## Vérification

Sur les quatre nœuds, après déploiement :

| | copies parasites / 5 min | CPU |
|---|---|---|
| asus | 0 | 2,2 % |
| acer | 0 | 3,5 % |
| gbs-p2 | 0 | — |
| gbs-p3 | 0 | — |

CPU **45 % → 2-4 %**. Propagation vérifiée dans les deux sens (asus → acer, puis acer → les trois autres), relue sur acer par `xclip` **et** par GTK, donc collable par une vraie application.

Le garde-fou en action, avant le redémarrage d'acer :

```
WARN  clipboard: horloge X11 lue comme du texte (456650760, horloge ≈ 456651998) — ignoré
WARN  clipboard: l'offre GTK a cessé de servir le texte — bascule sur xclip (26 octets)
WARN  clipboard_incoming: ignore sortie de sonde X11 reçue de gbs-p2 (45 octets)
```

## Ce qui relevait de la machine, pas du code

La sélection gelée d'acer **survivait à l'arrêt de l'agent** et résistait à un propriétaire `xclip` posé à la main : c'était sa session X qui était malade, pas PoolSync. Le redémarrage d'acer l'a effacée (`TARGETS` annonce à nouveau les 8 cibles et les sert).

Le tort de PoolSync n'était donc pas de causer l'état, mais d'en faire une tempête à l'échelle du pool. C'est cette partie qui est corrigée : la prochaine session qui partira en vrille sera détectée, tracée et contenue localement.

## Contexte de déploiement

- **gbs-p2 est en glibc 2.36** (Debian 12) ; asus compile en 2.39, son binaire y échoue (`GLIBC_2.39 not found`). acer et gbs-p3 (glibc 2.41) l'acceptent. Pour gbs-p2, compiler sur place — c'est l'objet de `deploy/build-portable-p2.sh`, et ce binaire-là est universel pour tout le pool.
- Toujours arrêter **`poolsync-watchdog.timer` en plus de `poolsync-agent`** : sinon le watchdog relance l'agent et l'installation échoue en « Text file busy ».
- Ne jamais lancer `pkill -f "poolsync-agent --config"` à travers `ssh` : le motif correspond à la ligne de commande `ssh` elle-même et tue la session. Utiliser `pkill -u zaza -x poolsync-agent`.

## Travaux liés, même série de commits

- **Ordre total du presse-papiers** (`clip_order.rs`) — les ~25 fenêtres temporelles concurrentes qui arbitraient entre deux contenus sont remplacées par une horloge de Lamport `(origin, seq)`. À deux nœuds elles tenaient par chance ; à quatre, un message ancien arrivé après un saut supplémentaire écrasait une copie récente.
- **`keep_formatting` rendu effectif** — l'option conservait bien le balisage à la lecture, mais `offer_text_payload` l'aplatissait aussitôt et n'offrait que la cible UTF8 ; la variante `ClipboardOffer::Rich` n'était jamais construite.
- **Purge du code mort** du chemin presse-papiers (33 → 15 avertissements), les 15 restants étant de l'échafaudage KVM/tray hors sujet.
