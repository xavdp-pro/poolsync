# Tableau de bord PoolSync

Interface web servie par le hub (`poolsync-hub --web-dir <dossier>`) : état du
pool, historique du presse-papiers, et éditeur de topologie (mosaïque des écrans).

## Construire

```bash
cd web
npm install
npm run build        # produit dist/
```

**Piège npm** : Tailwind 4 charge un binaire natif par plateforme, et npm a un
bug connu qui l'omet lors de l'installation ([npm/cli#4828]). Le build échoue
alors sur `Cannot find native binding`. Correctif :

```bash
npm install --no-save @tailwindcss/oxide-linux-x64-gnu@4.3.2
```

[npm/cli#4828]: https://github.com/npm/cli/issues/4828

Node 20 ou plus est recommandé (`vite` et `tailwindcss` le réclament) ; le build
passe tout de même en Node 18 avec un avertissement `EBADENGINE`.

## Déployer sur le hub

```bash
tar czf /tmp/web-dist.tgz -C dist .
scp /tmp/web-dist.tgz root@<hub>:/tmp/
ssh root@<hub> 'rm -rf /opt/poolsync/web/* && tar xzf /tmp/web-dist.tgz -C /opt/poolsync/web && systemctl restart poolsync-hub'
```

Le hub de production (gbs-p3) sert alors le tableau de bord sur
`http://10.87.78.22:9470/`. Le token du pool est demandé par l'interface pour
les appels d'API.

## Développer

```bash
npm run dev          # http://127.0.0.1:9471, proxy vers le hub
```
