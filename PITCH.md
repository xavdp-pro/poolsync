# PoolSync — Un seul bureau, plusieurs machines

## En une phrase

**PoolSync** fait la même chose que **Barrier** — un clavier, une souris, un presse-papiers partagés sur plusieurs écrans — mais **sans maître fixe** : la machine que vous utilisez *devient* le maître, où que vous soyez dans la pièce.

---

## Le même usage que Barrier

Barrier (ou Synergy) sert à **étaler un bureau sur plusieurs ordinateurs** reliés en réseau :

- **Serveur ou desktop** : peu importe, chaque machine a une interface graphique (XFCE, bureau classique).
- On installe un agent sur **chaque** poste du pool.
- On définit la **topologie** : qui est à gauche, à droite, au-dessus, en dessous de qui — comme des écrans d'un seul grand bureau.
- La souris **sort par le bord** d'un écran et **entre** sur le voisin ; le clavier suit ; le **presse-papiers est partagé** aussi (option `clipboardSharing`, activée par défaut).

Le pool peut faire **3, 5, 6, 7 machines** ou plus — selon ce que vous avez sur le bureau ou dans la pièce.

### Presse-papiers chez Barrier (nuance importante)

Barrier **partage bien le presse-papiers** entre les machines du pool — c'est documenté officiellement. En pratique :

| | Barrier |
|---|---------|
| **Texte** | Oui, cas d'usage principal |
| **Images** | Théoriquement prévu, mais **souvent défaillant sur Linux** (captures, PNG) ; lent ou instable sur grosses images |
| **Fiabilité** | Peut casser après reboot, VPN, ou entrer en conflit avec d'autres outils (clipman, RDP) |

PoolSync reprend le même principe, avec un presse-papiers **texte + images** plus robuste sur XFCE (GTK, webmail).

---

## Scénario concret : la pièce avec 6 ordinateurs

Imaginez une pièce ou un open space : **six ordinateurs** alignés sur un long bureau, de l'extrême gauche à l'extrême droite.

Vous êtes **tout à droite**, assis devant la machine sur laquelle vous travaillez au quotidien (mails, code, documents).

À **l'extrême gauche**, il y a un ordinateur que vous êtes en train de **finir de configurer** — installation, réglages, tests.

### Avec Barrier

- Il faut choisir **un maître fixe** et des **esclaves**.
- Le clavier et la souris ne pilotent les autres machines **que depuis le maître**.
- Pour configurer la machine de gauche pendant que vous travaillez à droite, il faut en pratique **vous asseoir devant le serveur maître** — ou accepter que le maître soit toujours la même machine, même quand ce n'est pas celle où vous êtes.

**Résultat** : vous bougez physiquement, ou vous subissez une config rigide.

### Avec PoolSync

- **Pas de maître imposé** : *devient maître ce qui est utilisé*.
- Vous restez **assis à droite** : votre machine devient le maître tant que vous y bougez la souris.
- Vous faites glisser la souris vers la gauche, écran après écran, jusqu'à l'ordi en configuration — **sans changer de chaise**.
- Vous installez l'agent **où vous voulez** (hub sur bs1, agents sur chaque poste) ; vous reliez la topologie une fois ; ensuite vous contrôlez **tout le parc** depuis **l'endroit où vous êtes** dans la pièce.

**Résultat** : un seul clavier/souris, un seul presse-papiers, **liberté de position** — le maître suit l'utilisateur, pas l'inverse.

---

## Le problème (au-delà de Barrier)

Même avec Barrier, au quotidien :

- presse-papiers **texte** OK en principe, mais **images** peu fiables sur Linux — recopier-coller manuel fréquent ;
- le presse-papiers casse (VPN, RDP, reboot, conflit clipman) ;
- instabilité après changement de réseau ou de VPN ;
- pas de tableau de bord : on ne voit pas qui est en ligne, qui est maître ;
- master figé alors que le travail se déplace d'un poste à l'autre.

---

## La solution

**PoolSync** = un **hub central léger** + un **agent** sur chaque machine + **interfaces graphiques** (systray, dashboard web).

| Fonction | Ce que ça fait |
|----------|----------------|
| **KVM multi-écrans** | Comme Barrier : souris/clavier aux bords, topologie mosaïque |
| **Maître dynamique** | La machine où vous bougez la souris devient le maître — pas de serveur « esclave » figé |
| **Presse-papiers partagé** | Texte et images en temps réel sur tout le pool |
| **Dashboard web** | Topologie, statut des nœuds, qui est en ligne / maître |
| **Systray** | État et réglages depuis chaque bureau (config + logs prévus) |
| **Résilience VPN** | Reconnexion auto quand **wg-bs1** revient |

---

## Pour qui ?

Toute personne ou équipe avec **plusieurs postes Linux** (desktop, portable, serveur avec bureau graphique) sur un même réseau privé (VPN WireGuard).

Exemples :

- **Pièce technique** : 5 à 7 machines sur un bureau, une seule souris/clavier.
- **Parc portable** : asus | acer | inspiron, hub sur **bs1**.
- **Mix serveur + desktop** : tant qu'il y a X11 et une session graphique, c'est dans le pool.

---

## PoolSync vs Barrier

| | Barrier | PoolSync |
|---|---------|----------|
| **Usage** | KVM + presse-papiers multi-machines | **Identique** |
| **Maître** | **Fixe** — un serveur, les autres esclaves | **Dynamique** — celui que vous utilisez |
| **Où vous travaillez** | Souvent devant le maître | **N'importe quel poste** du pool |
| **Topologie** | Fichier de config | Web + mosaïque modifiable |
| **Presse-papiers** | Partagé (texte ✅ ; images capricieuses sur Linux) | Texte + images robustes (GTK, webmail) |
| **VPN / reconnexion** | Fragile | Watchdog wg-bs1 |
| **Visibilité** | Aucun dashboard | Web + systray |
| **Stack** | C++ legacy | Rust, systemd |

---

## Architecture

```
[Poste 1] ←→ [Hub bs1 :9470] ←→ [Poste 2]
                 ↑
            [Poste 3 … N]
```

Topologie physique (exemple pièce) :

```
[Config] — [Dev] — [Test] — [Prod] — [Mail] — [Vous ▶]
  gauche                                              droite
```

1. Chaque agent se connecte au hub (WebSocket / VPN).
2. Vous êtes à droite → votre poste est maître.
3. Souris vers la gauche → vous pilotez la machine en config, puis revenez sans vous lever.
4. Collage sur n'importe quel poste → propagé à tout le pool.

---

## Stack

- **Rust** : `poolsync-agent`, `poolsync-hub`, `poolsync-core`
- **X11** : grab souris/clavier style Barrier, presse-papiers, injection
- **Web** : dashboard React (topologie, statut temps réel)
- **Déploiement** : systemd user + watchdog VPN

**Repo :** https://github.com/xavdp-pro/poolsync.git

---

## Pitch elevator (30 secondes)

> *« PoolSync, c'est Barrier refait pour la vraie vie. Même principe : plusieurs ordinateurs, un seul clavier, une seule souris, presse-papiers partagé — serveur ou desktop, avec interface graphique. Sauf qu'avec Barrier, il faut un maître fixe : vous êtes souvent coincé devant la machine serveur. Avec nous, **devient maître ce que vous utilisez**. Six ordis sur un bureau, vous êtes à droite en train de bosser, vous faites glisser la souris jusqu'à l'ordi en config à gauche — sans bouger de chaise. Hub léger sur votre VPN, pas de cloud. »*

---

## Version WhatsApp (collègue)

Copier-coller tel quel :

```
*PoolSync* — Barrier, mais sans maître figé

*Barrier*
• 1 machine = *serveur maître*
• les autres = *esclaves*
• clavier/souris pilotent tout *depuis le maître*
→ tu dois souvent t'asseoir devant le serveur, même si tu bosses ailleurs

*PoolSync*
• *pas de maître fixe*
• *devient maître* la machine où tu bouges la souris
• tu installes où tu veux, tu contrôles tout le parc *depuis le poste où tu es*

*Exemple*
6 ordis sur un bureau. Tu es à droite sur ton poste habituel. À gauche, un ordi en config.
→ Barrier : aller au maître ou accepter qu'il soit toujours le même
→ PoolSync : tu restes assis, souris vers la gauche, tu pilotes l'autre — le maître *c'est toi, là, maintenant*

+ presse-papiers partagé (texte + images Linux)
+ dashboard web + reconnexion VPN

https://github.com/xavdp-pro/poolsync.git
```
