#!/usr/bin/env bash
# Charge les secrets de déploiement depuis `.env` à la racine du dépôt.
#
# `.env` n'est jamais versionné (cf. .gitignore) : c'est le seul endroit où le
# token du pool doit vivre. Voir `.env.example` pour le gabarit.
#
# Usage, depuis un script de deploy/ :
#   source "$(dirname "$0")/load-env.sh"
#
# La variable d'environnement l'emporte sur le fichier, pour les usages
# ponctuels : POOLSYNC_TOKEN=xxx ./deploy/mon-script.sh

_poolsync_env_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
_poolsync_env_file="${POOLSYNC_ENV_FILE:-${_poolsync_env_root}/.env}"

if [[ -f "$_poolsync_env_file" ]]; then
  # `set -a` exporte tout ce que le fichier définit, sans avoir à répéter
  # `export` sur chaque ligne.
  set -a
  # shellcheck disable=SC1090
  source "$_poolsync_env_file"
  set +a
fi

if [[ -z "${POOLSYNC_TOKEN:-}" ]]; then
  cat >&2 <<EOF
POOLSYNC_TOKEN manquant.

Renseigne-le dans ${_poolsync_env_file} (voir .env.example) :

    cp .env.example .env && \$EDITOR .env

ou pour un appel ponctuel :

    POOLSYNC_TOKEN=xxx $0
EOF
  exit 1
fi
