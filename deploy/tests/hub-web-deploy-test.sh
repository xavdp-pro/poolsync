#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/../.." && pwd)"
SCRIPT="$ROOT/deploy/install-hub-gbs-p3.sh"

grep -F 'source "$ROOT/deploy/load-env.sh"' "$SCRIPT" >/dev/null
grep -F '(cd "$ROOT/web" && npm ci --silent && npm run build)' "$SCRIPT" >/dev/null
grep -F '[[ ! -s "$ROOT/web/dist/index.html" ]]' "$SCRIPT" >/dev/null
grep -F 'tar -C "$ROOT/web/dist" -cf - .' "$SCRIPT" >/dev/null
grep -F 'test -s "$stage/web/index.html"' "$SCRIPT" >/dev/null
grep -F 'Authorization: Bearer ${POOLSYNC_TOKEN}' "$SCRIPT" >/dev/null

if grep -Eq 'openssl rand|echo .*\$(TOKEN|POOLSYNC_TOKEN)' "$SCRIPT"; then
  echo "hub-web-deploy-test: secret généré ou affiché par le déploiement" >&2
  exit 1
fi

echo "hub-web-deploy-test: fresh Web bundle and authenticated health check required"
