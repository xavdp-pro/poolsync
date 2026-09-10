#!/usr/bin/env bash
# Compatibilité bs1 : utilise l'unique pipeline de déploiement sécurisé du hub.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
export HOST="${BS1:-bs1}"
export HUB_AGENT_URL="${HUB_AGENT_URL:-ws://10.24.42.1:9470/ws}"
exec "$ROOT/deploy/install-hub-gbs-p3.sh"
