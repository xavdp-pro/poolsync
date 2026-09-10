# Liaison pour contre-regard Claude — PoolSync v2 sans serveur obligatoire

## Mission

Tu interviens comme second regard indépendant sur PoolSync v2.

Analyse le code réellement présent dans ce dépôt, puis produis un rapport critique et concret. Ne suppose pas que les affirmations de ce document sont exactes : vérifie-les dans le code.

Pour cette première passe, ne modifie aucun fichier. Nous voulons d'abord un diagnostic d'architecture, de sécurité et de migration. Si tu proposes du code, limite-toi à de courts exemples illustratifs dans ton rapport.

## Objectif utilisateur

PoolSync doit permettre d'ajouter, retirer, éteindre ou rallumer des ordinateurs selon les besoins du moment.

La cible n'est pas simplement de déplacer le serveur sur une autre machine. La cible est qu'aucun ordinateur ne soit une machine de référence indispensable :

- chaque machine autorisée doit pouvoir rejoindre ou quitter le pool ;
- le presse-papiers doit continuer entre les pairs joignables ;
- ASUS et Acer utilisent le presse-papiers et le KVM ;
- gbs-p2 et gbs-p3 utilisent le presse-papiers, sans KVM ;
- l'arrêt de gbs-p3, ou de n'importe quel autre nœud, ne doit pas empêcher les autres de fonctionner ;
- après l'arrêt de toutes les machines, leur redémarrage dans un ordre quelconque doit reconstituer le pool ;
- le système doit fonctionner sur LAN et, lorsque nécessaire, à travers le VPN ;
- une machine retirée ou révoquée ne doit plus pouvoir lire ni injecter de données.

Un service facultatif de rendez-vous pour franchir plusieurs réseaux peut être acceptable, à condition qu'il ne soit ni une source de vérité unique ni un relais obligatoire pour le presse-papiers ou le KVM.

## Déploiement actuel

La version installée est PoolSync v2.0.0 sur quatre nœuds :

- `asus` : mode complet, presse-papiers et KVM ;
- `acer` : mode complet, presse-papiers et KVM ;
- `gbs-p2` : mode presse-papiers uniquement, session RDP ;
- `gbs-p3` : mode presse-papiers uniquement, session RDP ; héberge actuellement le hub.

Les connexions agent-hub et pair-à-pair utilisent WSS. Les pairs ont des jetons individuels et le contenu du presse-papiers peut être chiffré de bout en bout. Aucun secret de production n'est joint à ce document.

## Architecture v2 observée

La branche de travail est `clipboard-total-order`. Le dernier commit de base au moment de la liaison est `bc7e10d`. La v2 comporte encore de nombreuses modifications non commitées : il faut donc examiner le répertoire de travail, pas seulement `HEAD`.

Éléments déjà présents :

- `poolsync-agent/src/peer_mesh.rs` établit un mesh WebSocket direct entre pairs pour le presse-papiers ;
- les listeners des pairs peuvent utiliser TLS ;
- les connexions entrantes sont authentifiées par identité de nœud et jeton de pair ;
- les messages du presse-papiers peuvent être chiffrés de bout en bout ;
- une déduplication limite les doubles livraisons par le hub et le mesh ;
- la topologie exclut du graphe KVM les nœuds configurés en `clipboard_only` ;
- les URL et voisins sont actuellement fournis par la configuration des agents ;
- le hub conserve encore la topologie et expose les API de configuration et d'historique ;
- les agents établissent toujours une connexion persistante au hub ;
- plusieurs états et actions KVM semblent encore coordonnés par le hub ;
- l'interface et le diagnostic supposent encore l'existence d'un hub.

Le terme « P2P » ne doit donc pas être accepté sans nuance : le chemin de données du presse-papiers est partiellement pair-à-pair, mais le plan de contrôle reste centralisé.

## Fichiers prioritaires à examiner

- `poolsync-agent/src/agent.rs`
- `poolsync-agent/src/peer_mesh.rs`
- `poolsync-agent/src/network.rs`
- `poolsync-agent/src/state.rs`
- `poolsync-agent/src/kvm.rs`
- `poolsync-agent/src/tray.rs`
- `poolsync-agent/src/config_window.rs`
- `poolsync-core/src/lib.rs`
- `poolsync-core/src/topology.rs`
- `poolsync-hub/src/main.rs`
- `deploy/generate-security.sh`
- `deploy/install-agent.sh`
- `deploy/install-agent-local.sh`
- `deploy/install-hub-gbs-p3.sh`
- `README.md`
- `ROADMAP.md`

Inspecte aussi les tests et les autres fichiers qui deviennent pertinents pendant l'analyse.

## Questions auxquelles le rapport doit répondre

### 1. Dépendances centrales réelles

Énumère toutes les fonctionnalités qui cessent de fonctionner quand le hub est indisponible : découverte, présence, topologie, élection du maître KVM, historique, ordre des copies, révocation, interface, diagnostics, etc.

Pour chacune, indique le fichier et la fonction concernés.

### 2. Architecture sans nœud indispensable

Propose une architecture cible minimale et robuste comprenant :

- découverte locale automatique, par exemple mDNS ou mécanisme équivalent ;
- découverte à travers le VPN ou plusieurs sous-réseaux ;
- cache persistant des pairs connus ;
- authentification mutuelle ;
- ajout explicite d'une nouvelle machine ;
- révocation et rotation des identités ;
- convergence après partitions réseau ;
- redémarrage dans n'importe quel ordre ;
- absence de « premier nœud » permanent.

Précise si un protocole de gossip, une CRDT, une élection temporaire ou un simple ensemble signé de membres est réellement nécessaire. Évite la complexité sans bénéfice concret pour quatre à une dizaine de machines.

### 3. Cohérence du presse-papiers

Évalue le modèle d'ordre actuel et les risques suivants :

- deux copies concurrentes sur des nœuds différents ;
- reconnexion après partition ;
- boucles et doubles livraisons ;
- ancien contenu réapparaissant après le retour d'un nœud ;
- historique distribué et suppression de l'historique ;
- arbitrage déterministe sans horloge centrale fiable.

Recommande un modèle précis : identifiant de message, horloge logique ou hybride, règle de comparaison et durée de rétention.

### 4. KVM dynamique

Détermine comment ASUS et Acer peuvent partager le KVM sans hub permanent :

- possession du rôle maître ;
- bascule aux bords des écrans ;
- résolution d'une double prise de contrôle ;
- disparition brutale du maître ;
- exclusion garantie des nœuds `clipboard_only` ;
- comportement sûr après une partition réseau.

Le système doit privilégier la sécurité d'entrée : en cas d'ambiguïté, aucun nœud ne doit injecter des événements à tort.

### 5. Sécurité

Cherche notamment :

- usurpation d'identité d'un pair ;
- rejeu de messages ;
- compromission d'une clef de groupe partagée ;
- limites des jetons statiques ;
- vérification TLS et association certificat-identité ;
- propagation fiable d'une révocation sans serveur central ;
- fuite de contenu dans les logs, l'historique ou l'interface ;
- exposition excessive des listeners sur LAN ou VPN.

Propose un mécanisme d'enrôlement utilisable par une personne seule, sans infrastructure PKI lourde.

### 6. Migration incrémentale

Fournis un plan découpé en étapes testables qui conserve la v2 actuelle fonctionnelle pendant la transition. Chaque étape doit préciser :

- les composants touchés ;
- le comportement de compatibilité avec le hub actuel ;
- les tests unitaires et d'intégration indispensables ;
- le critère permettant de passer à l'étape suivante ;
- la méthode de retour arrière.

La dernière étape doit permettre d'arrêter gbs-p3 et son hub sans perdre le presse-papiers entre les autres machines.

## Scénarios d'acceptation à couvrir

Au minimum :

1. ASUS et Acer démarrent seuls et se retrouvent automatiquement.
2. gbs-p2 rejoint ensuite le pool et reçoit le presse-papiers, sans rejoindre le KVM.
3. gbs-p3 rejoint, puis s'éteint alors qu'il hébergeait auparavant le hub.
4. Les trois autres nœuds continuent à fonctionner.
5. Tous les nœuds s'éteignent, puis redémarrent dans un ordre différent.
6. Deux nœuds copient simultanément pendant une partition, puis le réseau revient.
7. Un ancien nœud révoqué tente de se reconnecter avec ses anciennes données d'accès.
8. Le LAN fonctionne alors que le VPN est coupé, puis l'inverse.
9. Le maître KVM disparaît brutalement pendant une session.
10. Une mise à jour progressive mélange temporairement agents actuels et nouveaux agents.

## Format attendu du rapport

Rends un rapport en français avec :

1. un verdict court sur l'état réellement P2P de la v2 ;
2. les constats classés par sévérité, avec références précises au code ;
3. l'architecture cible recommandée ;
4. les alternatives rejetées et pourquoi ;
5. le plan de migration incrémental ;
6. la matrice des scénarios de test ;
7. les questions ou hypothèses restant à trancher.

Signale clairement toute conclusion dont tu n'es pas certain.

## Règles de confidentialité et de collaboration

- Ne demande, ne copie et n'affiche aucun jeton, mot de passe, certificat privé ou clef de chiffrement déployée.
- Ne lis pas les répertoires de secrets locaux ou les configurations installées sur les machines.
- Travaille uniquement à partir du dépôt expurgé qui t'est fourni.
- Ne fais pas de commit et ne déploie rien pendant cette première analyse.
- Le rapport sera ensuite remis à Codex pour confrontation avec le code et décision commune.
