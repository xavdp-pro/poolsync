#!/usr/bin/env bash
# Compile une fois sur gbs-p2 (Debian glibc 2.36), puis déploie ce binaire
# compatible vers les hôtes ayant une glibc égale ou plus récente.
set -euo pipefail

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BUILD_HOST="${BUILD_HOST:-gbs-p2}"
BUILD_ROOT="${BUILD_ROOT:-$ROOT}"
TARGETS=("asus" "acer" "gbs-p3")

echo "==> Synchronisation des sources vers ${BUILD_HOST}"
rsync -az --exclude target --exclude .git "${ROOT}/" "${BUILD_HOST}:${BUILD_ROOT}/"

echo "==> Compilation unique sur ${BUILD_HOST}"
ssh "${BUILD_HOST}" "set -e; cd '${BUILD_ROOT}'; /root/.cargo/bin/cargo build --release -p poolsync-agent"

bundle_dir="$(mktemp -d)"
trap 'rm -rf "${bundle_dir}"' EXIT
portable_bin="${bundle_dir}/poolsync-agent"
scp "${BUILD_HOST}:${BUILD_ROOT}/target/release/poolsync-agent" "${portable_bin}"

echo "==> Installation sur ${BUILD_HOST}"
ssh "${BUILD_HOST}" 'set -e; install -o zaza -g zaza -m 755 "'$BUILD_ROOT'/target/release/poolsync-agent" /home/zaza/.local/bin/poolsync-agent; uid=$(id -u zaza); XDG_RUNTIME_DIR=/run/user/$uid DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$uid/bus runuser -u zaza -- systemctl --user restart poolsync-agent.service'

for host in "${TARGETS[@]}"; do
  echo "==> Installation sur ${host}"
  if [[ "$host" == "asus" ]]; then
    install -m 755 "${portable_bin}" "$HOME/.local/bin/poolsync-agent"
    systemctl --user restart poolsync-agent.service
  else
    scp "${portable_bin}" "${host}:/tmp/poolsync-agent.new"
    ssh "${host}" 'set -e; install -o zaza -g zaza -m 755 /tmp/poolsync-agent.new /home/zaza/.local/bin/poolsync-agent; rm -f /tmp/poolsync-agent.new; uid=$(id -u zaza); XDG_RUNTIME_DIR=/run/user/$uid DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/$uid/bus runuser -u zaza -- systemctl --user restart poolsync-agent.service'
  fi
done

echo "OK — binaire unique compilé sur p2 (glibc 2.36) et déployé."
