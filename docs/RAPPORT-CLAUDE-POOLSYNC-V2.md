# Contre-regard Claude — PoolSync v2

Réponse à `LIAISON-CLAUDE-POOLSYNC-V2.md`. Première passe, lecture seule.

| | |
|---|---|
| Dépôt | `poolsync-v1` |
| Branche | `clipboard-total-order` |
| Base | `bc7e10d` + modifications non commitées (répertoire de travail) |
| Version | 2.0.0 |
| Date | 10/09/2026 |
| Portée lue | ~14 700 lignes |
| Version en ligne | https://claude.ai/code/artifact/da8d20f9-8594-4da9-9f28-87f01bf85a28 |

Aucun fichier du dépôt n'a été modifié, aucun commit, aucun déploiement. Aucun jeton,
certificat privé ni clef déployée n'a été lu. Le seul matériel cryptographique manipulé
a été régénéré dans un répertoire temporaire pour vérifier le constat B2, puis détruit.

---

## 1. Verdict

**La v2 n'est pas pair-à-pair, et le mesh est muet sans hub.**

Le presse-papiers a bien un chemin de données direct entre pairs. Mais ce chemin ne
transporte jamais rien tant que le hub est injoignable, parce que le producteur
d'événements — la boucle de lecture du presse-papiers local — vit à l'intérieur de la
session hub.

`peer_mesh::spawn` est appelé indépendamment du hub (`main.rs:138`) : les liens directs
s'établissent, les écoutes montent, tout paraît sain. Mais `clipboard_poll_loop`, seul
émetteur de copies locales, n'est démarré qu'à `agent.rs:95` — après `wait_for_hub()`
(`agent.rs:50`), qui boucle indéfiniment, et après la poignée de main WebSocket. À chaque
perte du hub, `clip_task.abort()` (`agent.rs:153`) le tue.

Conséquence directe : **les scénarios 3, 4 et 5 échouent aujourd'hui**. Le mesh ne peut
relayer que du trafic né sur un nœud disposant d'une session hub vivante. Aucun ne l'a
quand gbs-p3 est éteint.

Le terme « P2P » mérite donc d'être retiré des trois plans à la fois, pas seulement du
plan de contrôle : le plan de contrôle est centralisé (topologie, maître KVM, historique,
révocation), le KVM est intégralement relayé par le hub, et le plan de données du
presse-papiers est *démarré* par le hub. Le mesh est un chemin décentralisé alimenté par
une source centralisée.

Deux nuances en sens inverse, à porter au crédit du code : le modèle d'ordre
`(origin, seq)` de `clip_order.rs` est le bon choix pour ce problème et il est
correctement testé ; et l'identité par nœud côté hub, avec rechargement à chaud et
révocation, est réelle et fonctionne. La v2 n'est pas à refaire — elle est à débrancher
du hub dans le bon ordre.

---

## 2. Constats par sévérité

Trois niveaux : **bloquant** pour ce qui empêche l'objectif « aucune machine
indispensable », **grave** pour ce qui casse la sécurité ou la correction sous conditions
atteignables, **moyen** pour ce qui coûte de la fiabilité ou de l'exploitabilité.

### Bloquants

#### B1 — Aucune copie locale n'est produite tant que le hub est absent

Le mesh est vivant mais n'a rien à diffuser. `run_agent` attend le hub en boucle infinie
avant de lancer quoi que ce soit d'autre, puis abandonne la tâche presse-papiers à chaque
déconnexion. Un pool sans hub est un pool sans presse-papiers, quel que soit l'état des
liens directs.

- `poolsync-agent/src/main.rs:138` — `peer_mesh::spawn` hors session hub (correct)
- `poolsync-agent/src/agent.rs:50` — `wait_for_hub()` bloque indéfiniment
- `poolsync-agent/src/agent.rs:95` — `clip_task` démarré après la connexion
- `poolsync-agent/src/agent.rs:153` — `clip_task.abort()` à chaque rupture

#### B2 — Les certificats de nœud ne peuvent pas valider les URL pair-à-pair

Le générateur donne au hub un SAN contenant son IP, mais aux nœuds un SAN `DNS:<nom>`
seulement — alors que toutes les `peer_url` déployées sont des IP, réécrites en `wss://`
à l'installation. La validation du nom d'hôte échoue donc systématiquement, et l'échec
est silencieux : `peer_outbound_loop` ne journalise qu'en `debug!` et retente
indéfiniment.

- `deploy/generate-security.sh:25` — SAN IP pour le hub
- `deploy/generate-security.sh:49` — `subjectAltName=DNS:$node` pour les nœuds
- `deploy/config/agent.asus.toml:20` — `peer_url = "ws://192.168.1.183:9472/ws"`
- `deploy/install-agent.sh:57` — réécriture `ws://` → `wss://`
- `poolsync-agent/src/peer_mesh.rs:330` — échec journalisé en `debug!`

Vérifié en régénérant le matériel avec le script du dépôt :

```
$ openssl verify -CAfile ca.crt -verify_ip 192.168.1.183 nodes/asus.crt
CN = asus
error 64 at 0 depth lookup: IP address mismatch
verification failed
```

Deux lectures possibles du terrain, à trancher : soit le mesh WSS est mort en silence et
tout le presse-papiers passe en réalité par le hub, soit les machines tournent encore en
`ws://` clair. **Je n'ai pas pu le déterminer** — la mission interdit de lire les
configurations installées. Le défaut du générateur, lui, est certain.

#### B3 — Le KVM passe intégralement par le hub

Le canal `peer_tx` ne transporte que le presse-papiers. `Input`, `SwitchTo` et
`MasterClaim` partent tous sur `out_tx`, qui est la WebSocket hub, et le hub les route.
Aucun chemin direct ASUS ↔ Acer n'existe pour le clavier et la souris.

- `poolsync-agent/src/kvm.rs:686, 703, 1038, 1047, 1059` — tous les émetteurs vers `out_tx`
- `poolsync-agent/src/agent.rs:98–102` — `out_tx` = canal hub
- `poolsync-hub/src/main.rs:1173–1210` — routage `Input` / `SwitchTo`

### Graves

#### G1 — Un seul nœud compromis permet d'usurper tous les autres

Le générateur écrit dans le TOML de chaque nœud une table `peer_tokens` contenant le
jeton de *tous* les autres. Or le jeton qu'un nœud présente en se connectant est
exactement celui que les autres attendent de lui : le secret est symétrique et connu de
tout le pool. Voler la configuration d'une machine suffit donc à se faire passer pour
n'importe quelle autre. La clef E2E est une clef de groupe unique, distribuée par les
mêmes fichiers : le même vol donne la lecture et la forge de tout le presse-papiers.

- `deploy/generate-security.sh:76–79` — `peer_tokens` = tous les jetons du pool
- `poolsync-agent/src/peer_mesh.rs:243–256` — `config_accepts_peer_token`, comparaison symétrique
- `poolsync-core/src/lib.rs:66–72` — `e2e_key` unique de groupe

#### G2 — L'ordre total est falsifiable : `origin` et `seq` ne sont pas authentifiés

`decrypt_clipboard` vérifie bien que les métadonnées extérieures correspondent à celles
scellées dans le chiffré — c'est un bon réflexe. Mais rien ne lie ces métadonnées à un
émetteur : tout détenteur de la clef de groupe fabrique un message signé
`origin: "asus"` avec le `seq` de son choix. L'ordre total repose donc sur l'honnêteté de
tous les membres.

- `poolsync-core/src/lib.rs` — aucune signature d'origine
- `poolsync-agent/src/clip_order.rs:59–77` — `accept_incoming` fait confiance à `(origin, seq)`

#### G3 — Un `seq` extrême casse l'horloge de façon permanente

`bump` calcule `current.max(observed) + 1` sans plafond. Un message portant
`seq = u64::MAX` — hostile, ou simplement émis par une machine dont l'horloge est très en
avance — fait déborder l'addition. Le profil release n'active pas les contrôles de
débordement : l'horloge repasse à 0, et toutes les copies locales du nœud perdent ensuite
contre tout ce qui arrive. Le presse-papiers reste cassé jusqu'au redémarrage de l'agent.

- `poolsync-agent/src/clip_order.rs:79–93` — `fn bump`, pas de borne sur `observed`
- `Cargo.toml` — aucun `[profile.release]`, donc `overflow-checks = false`

#### G4 — La compatibilité « ancien agent » est une porte de rétrogradation ouverte

`accept_incoming` renvoie `true` sans aucun contrôle dès que `seq == 0` ou que `origin`
est vide, et sans mettre à jour `last`. N'importe quel émetteur peut donc court-circuiter
l'ordre total en omettant deux champs, et gagner à tous les coups. C'est raisonnable
pendant une migration ; ça ne l'est plus une fois les quatre agents à niveau, et rien ne
date aujourd'hui la fermeture de cette porte.

- `poolsync-agent/src/clip_order.rs:60–62` — `if seq == 0 || origin.is_empty() { return true }`

#### G5 — L'exclusion des nœuds `clipboard_only` n'est pas garantie

La règle demandée est qu'en cas d'ambiguïté aucun nœud n'injecte. Le code fait l'inverse :
`target_kvm_enabled` renvoie `true` quand le nœud cible est absent de la topologie. Or la
topologie ne vient que du hub, par `TopologyUpdate`, et n'est jamais persistée côté agent.
Avant le premier message du hub — donc à chaque démarrage, et indéfiniment si le hub est
absent — `neighbor_of` retombe sur les voisins de la configuration locale et considère
gbs-p2 et gbs-p3 comme pilotables au clavier et à la souris.

Le défaut sûr doit être `false`, et le refus doit être appliqué **côté récepteur** : un
nœud en `clipboard_only` devrait ignorer tout message `Input` quoi qu'en dise l'émetteur.

- `poolsync-agent/src/state.rs:566–573` — `.unwrap_or(true)`
- `poolsync-agent/src/kvm.rs:971–989` — repli sur les voisins de config
- `poolsync-agent/src/state.rs` — `topology: RwLock<Option<…>>`, mémoire seule
- `poolsync-agent/src/agent.rs` — `Message::Input` accepté sans vérifier le mode local

#### G6 — La CA PoolSync est installée dans le magasin de confiance système

Les deux scripts d'installation déposent `ca.crt` dans `/usr/local/share/ca-certificates`
puis lancent `update-ca-certificates`. Cette CA — Ed25519, dix ans, sans `nameConstraints`
— devient alors de confiance pour *tout* ce qui tourne sur la machine : curl, navigateurs,
gestionnaires de paquets. Une fuite de `ca.key` ne compromettrait pas seulement PoolSync,
elle donnerait une interception universelle sur les quatre postes.

Un pool de machines n'a pas besoin de la validation par nom d'hôte du Web : un magasin de
racines dédié à PoolSync, ou mieux un épinglage de clef publique, ferme ce risque et règle
au passage le constat B2.

- `deploy/install-agent.sh:68–69`, `deploy/install-agent-local.sh:82–83` — `update-ca-certificates`
- `deploy/generate-security.sh:20–22` — CA 3650 jours, sans contrainte de nom

#### G7 — La révocation ne se propage pas sur le mesh

Côté hub, la révocation est réelle et rapide : `node-tokens.json` est relu toutes les cinq
secondes et la session est coupée. Côté pair-à-pair, il n'existe rien : les `peer_tokens`
vivent dans le TOML local de chaque agent. Révoquer une machine impose donc d'éditer les
configurations des trois autres et de les redémarrer à la main — et de tourner la clef E2E
de groupe partout, puisqu'elle est unique. Le scénario 7 n'est satisfait que sur le chemin
hub, c'est-à-dire précisément le chemin qu'on veut supprimer.

- `poolsync-hub/src/main.rs:293–300` — `node_identity_still_valid`
- `poolsync-hub/src/main.rs:920–925` — `credential_check` toutes les 5 s
- `poolsync-agent/src/peer_mesh.rs:243–256` — jetons pairs figés dans la config

#### G8 — La PKI par nœud existe mais ne sert jamais à l'authentification

Chaque nœud reçoit un certificat et une clef, mais le listener est construit
`with_no_client_auth()` et les certificats portent `extendedKeyUsage = serverAuth` seul.
L'identité repose donc entièrement sur un porteur partagé (G1), alors que le matériel
nécessaire à une authentification mutuelle est déjà généré et déjà distribué. C'est la
correction au meilleur rapport effort/gain du dossier.

- `poolsync-agent/src/peer_mesh.rs:221` — `.with_no_client_auth()`
- `deploy/generate-security.sh:49` — `extendedKeyUsage=serverAuth`

### Moyens

#### M1 — E2E et historique hub s'excluent mutuellement, sans que ce soit dit

En clair, le hub range le contenu complet et une vignette dans son historique. En mode
chiffré, la branche `EncryptedClipboard` se contente de relayer : ni historique, ni aperçu,
ni vignette. Activer l'E2E vide donc le menu du systray et le tableau de bord, sans message
d'avertissement ni mention dans la documentation. À décider explicitement : historique
local par nœud (le cache existe déjà) ou historique hub en clair.

- `poolsync-hub/src/main.rs:1140` — historisation du clair
- `poolsync-hub/src/main.rs:1156–1160` — relais chiffré sans historisation
- `poolsync-agent/src/clip_cache.rs` — cache local, déjà présent

#### M2 — Le contenu copié part dans journald

La trace de diagnostic ajoutée après la tempête du 29/08 journalise en `info!` les quarante
premiers caractères de chaque copie. Un mot de passe ou un jeton copié se retrouve donc
dans le journal système, lisible par tout ce qui lit journald, et conservé selon la
rétention de la machine. À passer en `debug!`, ou à réduire à une empreinte tronquée.

- `poolsync-agent/src/agent.rs:445` — `preview={:?}`, 40 caractères en clair

#### M3 — La déduplication du mesh s'efface d'un bloc et ne survit pas au redémarrage

Au-delà de 4096 identifiants, `trim_seen_messages` vide entièrement l'ensemble plutôt que
d'évincer les plus anciens : juste après la purge, un message déjà relayé peut l'être à
nouveau, ce qui rouvre la porte aux boucles que la v2 cherchait à fermer. L'ensemble
n'étant qu'en mémoire, un redémarrage autorise aussi le rejeu d'anciens messages. Un anneau
LRU persisté règle les deux.

- `poolsync-agent/src/peer_mesh.rs:143–147` — `seen.clear()`

#### M4 — Aucune découverte, aucun cache de pairs

URL, voisins, jetons et clef : tout est figé dans les fichiers TOML produits à
l'installation. Ajouter une machine impose de régénérer et de redéployer les
configurations de toutes les autres. Les exigences « découverte locale automatique »,
« cache persistant des pairs connus » et « ajout explicite d'une nouvelle machine » ne sont
couvertes par aucun code aujourd'hui.

- `deploy/config/agent.*.toml` — adresses et voisins en dur
- `poolsync-agent/src/peer_mesh.rs:40–48` — voisins lus depuis la config seule

#### M5 — Le maître KVM est « le dernier qui a parlé »

Le protocole transporte bien un horodatage dans `MasterClaim`, mais le hub l'ignore
explicitement (`ts: _`) et écrase le propriétaire courant à chaque revendication. Deux
nœuds qui réclament en même temps se disputent sans arbitrage, et la disparition brutale du
maître ne libère rien tant que sa session WebSocket n'est pas tombée. Le champ nécessaire à
un arbitrage déterministe existe déjà : il n'est pas lu.

- `poolsync-hub/src/main.rs:1162–1172` — `Message::MasterClaim { node, ts: _ }`
- `poolsync-hub/src/main.rs` — `unregister_node`, libération à la déconnexion seulement

#### M6 — Les liens morts restent dans la table des pairs

La boucle centrale insère chaque lien établi mais ne retire jamais rien à la fin d'une
session. Les envois vers un pair disparu échouent silencieusement, ce qui est sans
conséquence fonctionnelle, mais l'agent n'a de ce fait aucune notion utilisable de « qui
est joignable » — et il n'en a aucune autre par ailleurs, la présence étant une information
exclusivement hub.

- `poolsync-agent/src/peer_mesh.rs:108–111` — `peers.insert`, sans retrait symétrique

---

## 3. Dépendances centrales réelles (question 1)

Relevé des fonctions dont le comportement change ou cesse en l'absence du hub.

| Fonctionnalité | État sans hub | Fichier · fonction |
|---|---|---|
| **Émission du presse-papiers** | Morte — la boucle de lecture n'est jamais lancée | `agent.rs` · `run_agent` → `clipboard_poll_loop` |
| Réception du presse-papiers | Chemin mesh fonctionnel, mais plus personne n'émet | `peer_mesh.rs` · `serve_peer_session` |
| **Topologie** | Perdue — jamais persistée côté agent, jamais reconstruite | `state.rs` · `set_topology` / `topology` |
| **Maître KVM** | Aucune élection possible | `hub/main.rs` · `handle_message` (`MasterClaim`) |
| **Bascule aux bords** | Morte — `SwitchTo` passe par le hub | `kvm.rs` · `switch_to`, `send_master_claim` |
| **Injection clavier/souris** | Morte — `Input` routé par le hub | `kvm.rs` · `send_key`, `send_mouse_absolute` |
| Présence des nœuds | Inexistante hors hub | `hub/main.rs` · `register_node` / `unregister_node` |
| Découverte | Statique par configuration, inchangée | `peer_mesh.rs` · `spawn` |
| Historique presse-papiers | Dégradé au cache local (repli présent) | `clipboard_history.rs` · `fetch_history` |
| Reprise d'une entrée d'historique | Morte — route HTTP hub | `clipboard_history.rs` · `/api/clipboard/pick` |
| Purge de l'historique | Morte — route HTTP hub | `clipboard_history.rs` · `clear`, `delete` |
| **Révocation** | Morte sur le mesh, vivante sur le hub seul | `hub/main.rs` · `node_identity_still_valid` |
| Édition de la configuration | Morte — `GET`/`POST /api/topology` | `config_window.rs` · `fetch/save` |
| Test visuel des bords | Mort — commande hub | `hub/main.rs` · `api_edges_show` |
| Tableau de bord web | Mort | `web/`, `hub/main.rs` · `api_status` |
| Ordre des copies | **Correct par construction** — `(origin, seq)` voyage avec le message | `clip_order.rs` · `accept_incoming` |

La dernière ligne est la bonne nouvelle du tableau : l'ordre total ne dépend d'aucune
horloge centrale. C'est la brique sur laquelle le reste de la décentralisation peut
s'appuyer.

---

## 4. Architecture cible (questions 2 à 5)

Pour quatre à dix machines, ni gossip, ni CRDT, ni consensus. Le bon niveau de complexité
tient en cinq décisions.

### a — Identité : une clef par machine, une liste de membres signée

Chaque nœud possède une paire Ed25519 — la PKI existe déjà, il suffit de la réutiliser au
lieu de la laisser inerte. Chaque message de presse-papiers est signé par son origine, ce
qui rend `(origin, seq)` infalsifiable et ferme G2.

L'appartenance au pool est un fichier `members.json` : nom, clef publique, numéro d'époque,
le tout signé par une clef d'administration conservée hors ligne — celle que le générateur
produit déjà sous le nom `ca.key`. C'est l'« ensemble signé de membres » évoqué dans la
mission, et il suffit. Il ferme G1, G7 et une partie de G8 d'un seul mécanisme.

- **Enrôlement par une personne seule** : sur la nouvelle machine, `poolsync enroll`
  affiche une empreinte courte et un code à six chiffres ; sur une machine déjà membre,
  `poolsync admit <code>` vérifie l'empreinte de visu, ajoute l'entrée, incrémente l'époque
  et signe. Modèle SSH ou Syncthing — aucune infrastructure PKI à tenir.
- **Révocation** : retirer l'entrée, incrémenter l'époque, re-signer. Le fichier se propage
  par le mesh lui-même ; chaque nœud accepte toute époque strictement supérieure et
  correctement signée. Convergence sans serveur, y compris après partition.
- **Rotation** : la nouvelle clef est publiée dans une nouvelle époque, l'ancienne reste
  acceptée jusqu'à l'époque suivante. Même mécanique que les `previous_tokens`
  d'aujourd'hui, mais sans fichier à recopier sur chaque machine.

### b — Transport : mTLS, magasin dédié, épinglage

Ajouter `clientAuth` aux certificats, remplacer `with_no_client_auth()` par un vérificateur
de certificat client dont la racine est la CA PoolSync, et — point essentiel — sortir cette
CA du magasin système au profit d'un magasin propre à l'agent. La validation se fait alors
par clef publique épinglée plutôt que par nom d'hôte, ce qui règle B2 sans avoir à courir
après les SAN à chaque changement d'adresse IP.

### c — Découverte : trois voies cumulatives

1. **Cache persistant des pairs** (`~/.local/state/poolsync/peers.json`) : dernière adresse
   connue et date, essayé en premier. C'est ce qui fait fonctionner le redémarrage dans un
   ordre quelconque, et c'est aussi le moins cher à écrire.
2. **mDNS** `_poolsync._tcp` sur le LAN, publiant le nom et l'empreinte de clef — jamais un
   secret. Couvre ASUS et Acer qui se retrouvent seuls.
3. **Liste statique VPN**, c'est-à-dire ce qui existe déjà, pour les sous-réseaux que mDNS
   ne franchit pas.

Le service de rendez-vous évoqué dans la mission ne devient nécessaire qu'au-delà de ces
trois voies. S'il est un jour ajouté, il doit rester un annuaire d'adresses signées :
jamais un relais, jamais une source de vérité.

### d — Presse-papiers : garder Lamport, le borner

Le modèle actuel est le bon et il est bien testé. Trois correctifs suffisent :

- borner `observed` dans `bump` — rejeter tout `seq` supérieur à `now_ms + 5 min` — ce qui
  ferme G3 ;
- supprimer la porte `seq == 0` à une étape datée du plan, ce qui ferme G4 ;
- lier `(origin, seq)` à la signature du point (a), sans quoi l'ordre reste falsifiable.

**Règle de comparaison** : inchangée — `(seq, origin)` en ordre lexicographique, le nom du
nœud départageant les égalités. Elle est déterministe et identique partout, ce qui est
exactement ce qu'on demande d'un arbitrage sans horloge centrale.

**Rétention** : anneau LRU de 4096 `msg_id`, persisté sur disque, plus un rejet des `seq`
antérieurs d'une heure au dernier appliqué. Les deux ensemble ferment M3 et empêchent le
contenu ancien de réapparaître au retour d'un nœud. L'historique reste local à chaque
nœud : `clip_cache` le fait déjà.

**Deux copies concurrentes pendant une partition** : les deux côtés convergent vers la
copie de `seq` le plus élevé dès la reconnexion, ce qui signifie qu'une des deux copies est
perdue. C'est le comportement correct pour un presse-papiers — un registre à écrivain
unique par instant — mais il vaut mieux l'assumer explicitement que le découvrir : la copie
perdue doit rester récupérable dans l'historique local, ce qui suppose que l'historique ne
soit plus hub.

### e — KVM : un bail court, et le refus par défaut

Pas d'élection, un **bail** de possession du clavier et de la souris : trois secondes,
renouvelé chaque seconde par son détenteur, diffusé sur le mesh. Un nœud n'injecte que s'il
détient un bail valide ; il n'accepte d'injection que du détenteur du bail.

- **Disparition brutale du maître** : le bail expire seul en trois secondes. Le premier
  nœud sur lequel l'utilisateur bouge physiquement en prend un nouveau. Aucun quorum,
  aucune détection de panne à écrire.
- **Double prise de contrôle** : le plus grand `(époque, ts, nom)` l'emporte, l'autre
  relâche immédiatement — la même règle lexicographique que le presse-papiers, ce qui évite
  d'avoir deux modèles d'arbitrage à tenir dans la tête.
- **Partition réseau** : chaque côté peut avoir un maître. C'est acceptable ici, puisque
  seuls ASUS et Acer sont concernés et qu'une partition entre eux signifie qu'ils ne se
  pilotent de toute façon plus.
- **Nœuds `clipboard_only`** : refus en dur côté récepteur — `mode == ClipboardOnly`
  implique ignorer tout `Input`, quoi qu'en dise l'émetteur. C'est la seule formulation qui
  tienne la promesse « exclusion garantie », parce qu'elle ne dépend d'aucune information
  distante.

### f — Hub : rétrogradé en service facultatif

Il garde le tableau de bord et un historique long si on le souhaite. Il ne détient plus ni
la topologie de référence, ni le maître KVM, ni la liste d'appartenance. Son arrêt devient
un non-événement.

---

## 5. Alternatives rejetées

| Option | Pourquoi elle est écartée |
|---|---|
| **CRDT sur le presse-papiers** | Le presse-papiers est un registre à écrivain unique par instant, pas une structure à fusionner. Un registre « dernier écrivain gagne » est déjà exactement ce que produit l'horloge de Lamport en place. Coût sans bénéfice. |
| **Gossip type SWIM** | Justifié à partir de plusieurs dizaines de nœuds. À dix, une liste de membres signée et des liens directs complets — 45 liens au maximum — sont plus simples, plus prévisibles, et surtout déboguables à la lecture d'un journal. |
| **Raft ou consensus pour le maître KVM** | Demande un quorum, donc cesse de fonctionner dès que deux machines sur quatre sont éteintes — précisément le scénario que la mission veut couvrir. Le bail ne demande aucun quorum. |
| **Élection permanente d'un nœud « hub »** | Recrée une machine de référence sous un autre nom. La mission l'exclut explicitement, et l'expérience de gbs-p3 montre ce que ça coûte. |
| **Hub conservé comme source de vérité, mais répliqué** | Double la complexité pour conserver le défaut qu'on cherche à supprimer. Réplication, cohérence et bascule à écrire et à tester, sans jamais atteindre « aucune machine indispensable ». |
| **DHT ou libp2p** | Dépendance considérable, et traversée de NAT inutile sur un LAN et un WireGuard maîtrisés. La surface de code dépasserait celle de PoolSync entier. |

---

## 6. Plan de migration incrémental

La numérotation encode une dépendance réelle : chaque étape suppose la précédente franchie
et validée. Les étapes 0 à 2 apportent l'essentiel du bénéfice pour un coût faible ;
l'arrêt de gbs-p3 devient possible à l'étape 8.

### Étape 0 — Corriger le générateur de certificats

Ajouter aux SAN des nœuds leurs adresses IP LAN et VPN, comme le script le fait déjà pour
le hub. Correction d'une ligne, sans laquelle tout ce qui suit se déploie sur un mesh qui
ne se connecte pas.

- **Composants** : `deploy/generate-security.sh`
- **Compatibilité hub** : totale, aucun changement de protocole
- **Tests** : étendre `security-v2-test.sh` avec `openssl verify -verify_ip` sur les IP
  réellement présentes dans les `peer_url`
- **Critère de passage** : les journaux des quatre agents montrent `peer mesh → <nœud>` sur
  chaque lien
- **Retour arrière** : réinstaller les certificats précédents

### Étape 1 — Découpler la boucle presse-papiers du hub

Sortir `clipboard_poll_loop` de `run_agent` et la lancer depuis `main`, à côté de
`peer_mesh::spawn`. La session hub devient un simple abonné supplémentaire du même flux, au
lieu d'en être le propriétaire. C'est le correctif décisif : à lui seul il rend le
presse-papiers survivant à l'arrêt du hub.

- **Composants** : `agent.rs`, `main.rs`, `clipboard.rs`
- **Compatibilité hub** : totale — le hub reçoit les mêmes messages, simplement plus par le
  même chemin de code
- **Tests** : intégration sur le banc — couper le hub, copier sur desk-a, vérifier
  l'arrivée sur desk-b. Unitaire : la boucle démarre sans `hub_url` joignable
- **Critère de passage** : scénarios 3 et 4 verts sur le banc LXD
- **Retour arrière** : réglage `clipboard_requires_hub = true` conservé une version, puis
  retiré

### Étape 2 — Persister topologie et pairs côté agent

Écrire la dernière topologie reçue et les adresses des pairs joignables dans
`~/.local/state/poolsync/`, et les relire au démarrage. Basculer `target_kvm_enabled` sur
un défaut `false`, et refuser `Input` côté récepteur quand le mode local est
`clipboard_only`.

- **Composants** : `state.rs`, `agent.rs`, `kvm.rs`
- **Compatibilité hub** : le hub reste prioritaire quand il est présent ; le cache ne sert
  qu'en son absence
- **Tests** : unitaire — `target_kvm_enabled` renvoie `false` pour un nœud inconnu.
  Intégration : redémarrage sans hub, la topologie est retrouvée
- **Critère de passage** : aucun événement d'entrée ne parvient à gbs-p2 ni gbs-p3, hub
  coupé comme allumé
- **Retour arrière** : supprimer le fichier d'état, le comportement redevient celui
  d'aujourd'hui

### Étape 3 — Durcir l'ordre du presse-papiers

Borner `bump`, remplacer la purge par un anneau LRU persisté, et ajouter le rejet des `seq`
trop anciens. Fermer la porte `seq == 0` par un réglage `accept_legacy_clipboard`, encore
vrai par défaut à cette étape.

- **Composants** : `clip_order.rs`, `peer_mesh.rs`
- **Compatibilité hub** : totale ; le `pick` du hub émet déjà un `seq` valide
- **Tests** : unitaire — `seq = u64::MAX` n'altère pas l'horloge ; un identifiant évincé du
  LRU n'est pas re-relayé. Propriété : deux copies concurrentes convergent chez les quatre
  nœuds
- **Critère de passage** : scénario 6 vert, et campagne de repos de 24 h sans copie fantôme
- **Retour arrière** : le réglage `accept_legacy_clipboard` reste basculable

### Étape 4 — mTLS et magasin de confiance dédié

Ajouter `clientAuth` aux certificats, activer le vérificateur de certificat client sur les
listeners, et retirer la CA PoolSync du magasin système au profit d'un magasin propre à
l'agent.

- **Composants** : `peer_mesh.rs`, `generate-security.sh`, scripts d'installation
- **Compatibilité hub** : le hub garde son TLS actuel ; les deux modes de listener pair
  coexistent une version
- **Tests** : un client sans certificat est refusé ; un certificat signé par une autre CA
  est refusé ; `update-ca-certificates` n'est plus appelé
- **Critère de passage** : les quatre liens montent en mTLS, et la CA PoolSync est absente
  du magasin système
- **Retour arrière** : réglage `peer_require_client_cert = false`

### Étape 5 — Liste de membres signée, enrôlement et révocation

Introduire `members.json` signé et versionné par époque, les commandes `enroll` et `admit`,
la propagation par le mesh, et la signature d'origine des messages de presse-papiers. Les
`peer_tokens` deviennent un repli, puis disparaissent.

- **Composants** : `poolsync-core`, `peer_mesh.rs`, nouveau module d'appartenance, CLI
- **Compatibilité hub** : le hub continue d'accepter `node-tokens.json` ; les deux systèmes
  cohabitent
- **Tests** : un nœud révoqué est refusé par les trois autres sans redémarrage ; une époque
  inférieure est ignorée ; une signature invalide est rejetée
- **Critère de passage** : scénario 7 vert *hub éteint*
- **Retour arrière** : réactiver `peer_tokens` dans les configurations conservées

### Étape 6 — Bail KVM sur le mesh

Porter `MasterClaim`, `SwitchTo` et `Input` sur les liens directs, avec le bail de trois
secondes et l'arbitrage `(époque, ts, nom)`. Ne concerne qu'ASUS et Acer, ce qui limite le
périmètre de risque.

- **Composants** : `kvm.rs`, `peer_mesh.rs`, `poolsync-core`
- **Compatibilité hub** : le hub relaie encore pour les agents non migrés ; la
  déduplication par identifiant évite la double injection
- **Tests** : le bail expire et se reprend en moins de 4 s après arrêt brutal ; deux
  revendications simultanées convergent vers un seul maître ; aucune injection sans bail
- **Critère de passage** : scénario 9 vert, et aucun événement parasite pendant une heure
  d'usage réel
- **Retour arrière** : réglage `kvm_transport = "hub"`

### Étape 7 — Découverte mDNS et cache de pairs

Publier et résoudre `_poolsync._tcp` sur le LAN, en n'annonçant que le nom et l'empreinte
de clef. Combiner avec le cache d'adresses de l'étape 2 et la liste statique VPN.

- **Composants** : nouveau module de découverte, `peer_mesh.rs`
- **Compatibilité hub** : sans objet — le hub n'a jamais participé à la découverte
- **Tests** : deux nœuds sans configuration croisée se trouvent sur le LAN ; un nœud non
  membre annoncé en mDNS est refusé à la poignée de main
- **Critère de passage** : scénarios 1, 2 et 8 verts
- **Retour arrière** : réglage `discovery_mdns = false`, retour aux adresses de
  configuration

### Étape 8 — Rendre le hub facultatif, puis éteindre gbs-p3

Rendre `hub_url` optionnel, déplacer l'historique et son interface côté agent, et retirer
le tableau de bord de la boucle de fonctionnement. Éteindre gbs-p3 et vérifier que rien ne
change pour les trois autres.

- **Composants** : `agent.rs`, `clipboard_history.rs`, `config_window.rs`, `tray.rs`
- **Compatibilité hub** : le hub reste démarrable pour le tableau de bord, sans qu'aucun
  agent n'en dépende
- **Tests** : suite complète, agents démarrés sans `hub_url` ; l'historique et le systray
  fonctionnent hors hub
- **Critère de passage** : les dix scénarios verts, gbs-p3 éteint
- **Retour arrière** : rallumer le hub — les agents s'y reconnectent sans changement de
  configuration

---

## 7. Matrice des scénarios de test

État évalué par lecture du code, pas par exécution sur les machines : la mission interdit
de toucher au déploiement. Les verdicts « échoue » sont déduits de chemins de code
vérifiés ; ils méritent une confirmation au banc.

| # | Scénario | Aujourd'hui | Rendu vert par | Test |
|---|---|---|---|---|
| 1 | ASUS et Acer démarrent seuls et se retrouvent | Partiel — seulement si les adresses configurées sont justes, et sans presse-papiers tant que le hub manque (B1) | Ét. 1, 7 | Intégration LXD, deux nœuds, hub absent |
| 2 | gbs-p2 rejoint, reçoit le presse-papiers, pas le KVM | **Échoue** — l'exclusion KVM n'est pas garantie sans topologie hub (G5) | Ét. 2 | Unitaire `target_kvm_enabled` + intégration |
| 3 | gbs-p3 rejoint puis s'éteint, après avoir hébergé le hub | **Échoue** — B1 | Ét. 1 | Intégration : couper le hub, copier, vérifier |
| 4 | Les trois autres nœuds continuent | **Échoue** — B1, B3 | Ét. 1, 6 | Campagne de repos de 24 h, hub éteint |
| 5 | Extinction totale, redémarrage en ordre quelconque | **Échoue** — B1, plus perte de topologie | Ét. 1, 2, 7 | Six permutations de démarrage au banc |
| 6 | Deux copies pendant une partition, puis retour | **Correct** — l'ordre total tranche déjà | Consolidé en ét. 3 | Propriété : convergence des quatre nœuds |
| 7 | Un nœud révoqué se reconnecte | Partiel — refusé par le hub, accepté par les pairs (G7) | Ét. 5 | Révocation hub éteint, tentative de reconnexion |
| 8 | LAN sans VPN, puis l'inverse | Partiel — le repli d'URL existe, mais B2 le neutralise en WSS | Ét. 0, 7 | Coupure `wg-bs1` puis coupure LAN |
| 9 | Le maître KVM disparaît brutalement | **Échoue** — aucune expiration, libération à la déconnexion WebSocket (M5) | Ét. 6 | Coupure d'alimentation simulée, chronométrage du bail |
| 10 | Mise à jour progressive, agents mélangés | Partiel — la compatibilité existe, mais par une porte ouverte (G4) | Ét. 3, 5 | Banc mixte : deux agents v2, deux agents v3 |

---

## 8. Questions ouvertes et limites de cette passe

### Questions pour Xavier

- **Quel est l'état réel du transport pair-à-pair sur les quatre machines ?** C'est la
  question la plus importante du dossier. Si le mesh WSS échoue en silence (B2), le
  presse-papiers passe aujourd'hui entièrement par le hub, et la v2 est encore plus
  centralisée que ce document ne le dit. Un `ss -tlnp | grep 9472` et un journal en
  `RUST_LOG=debug` sur un agent tranchent en deux minutes.
- **L'E2E est-il activé en production ?** Si oui, l'historique du hub et le menu du systray
  devraient être vides (M1) — ce qui se constate immédiatement et confirmerait le constat.
- **Les quatre machines sont-elles synchronisées par NTP ?** L'horloge de Lamport est
  amorcée sur l'heure mur : une machine très en avance domine durablement l'ordre du pool,
  et une machine très en retard voit ses copies perdre systématiquement.
- **Faut-il conserver la compatibilité `seq == 0`** après la migration, sachant que c'est
  une porte de rétrogradation permanente ? Recommandation : la fermer à l'étape 3, avec un
  réglage de secours conservé une version.
- **Le service de rendez-vous multi-réseaux est-il vraiment nécessaire ?** Au vu du parc —
  LAN plus WireGuard maîtrisé — je penche pour non, et pour ne pas l'écrire tant qu'un
  besoin concret ne se présente pas.
- **gbs-p2 et gbs-p3 en session RDP** sont-ils censés participer un jour au KVM ? J'ai
  supposé que non, et l'architecture cible refuse l'injection en dur pour eux.

### Ce que cette passe n'a pas couvert

- `clipboard.rs` — 2 777 lignes — n'a pas été lu intégralement. Les constats sur le
  presse-papiers portent sur les couches d'ordre et de transport, pas sur les subtilités
  X11 et GTK de la prise de sélection, où d'autres problèmes peuvent se cacher.
- `kvm.rs` a été parcouru par recherche ciblée et lecture de ses sections décisives, pas
  ligne à ligne. La géométrie multi-écrans et la cartographie des coordonnées n'ont pas été
  auditées.
- L'interface web, `tray.rs` et `config_window.rs` n'ont été examinés que pour leurs
  dépendances au hub.
- Aucune vérification sur les machines déployées, conformément aux règles de la mission :
  les conclusions portant sur l'état du terrain sont explicitement marquées comme
  incertaines.

### Si une seule chose devait être faite cette semaine

**L'étape 1.** Sortir `clipboard_poll_loop` de la session hub représente quelques dizaines
de lignes, ne change rien au protocole, et transforme à elle seule un pool où l'arrêt de
gbs-p3 tue le presse-papiers en un pool où il ne le tue plus. Tout le reste peut suivre à
son rythme.
