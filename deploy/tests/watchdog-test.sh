#!/usr/bin/env bash
set -euo pipefail

TEST_DIR="$(mktemp -d)"
trap 'rm -rf "$TEST_DIR"' EXIT
FAKE_BIN="$TEST_DIR/bin"
TEST_HOME="$TEST_DIR/home"
CALLS="$TEST_DIR/calls"
mkdir -p "$FAKE_BIN" "$TEST_HOME/.config/poolsync" "$TEST_HOME/.cache/poolsync" "$CALLS"

cat > "$TEST_HOME/.config/poolsync/agent.toml" <<'TOML'
node = "desk-test"
hub_url = "ws://hub.test/ws"
token = "test-token"
TOML
printf '0\n' > "$TEST_HOME/.cache/poolsync/wg-bs1-up"
printf '1\n' > "$TEST_HOME/.cache/poolsync/hub-up"
printf '2\n' > "$TEST_HOME/.cache/poolsync/node-misses"

cat > "$FAKE_BIN/timeout" <<'SH'
#!/usr/bin/env bash
exit 0
SH
cat > "$FAKE_BIN/ip" <<'SH'
#!/usr/bin/env bash
exit 1
SH
cat > "$FAKE_BIN/systemctl" <<'SH'
#!/usr/bin/env bash
if [[ " $* " == *" restart "* ]]; then
  printf '%s\n' "$*" >> "$POOLSYNC_TEST_CALLS/restarts"
fi
exit 0
SH
cat > "$FAKE_BIN/curl" <<'SH'
#!/usr/bin/env bash
printf '%s\n' "$@" > "$POOLSYNC_TEST_CALLS/curl-args"
printf '{"nodes":[{"name":"desk-test","online":true}]}\n'
SH
cat > "$FAKE_BIN/logger" <<'SH'
#!/usr/bin/env bash
exit 0
SH

chmod +x "$FAKE_BIN"/*
HOME="$TEST_HOME" PATH="$FAKE_BIN:$PATH" POOLSYNC_TEST_CALLS="$CALLS" \
  bash "$(cd "$(dirname "$0")/.." && pwd)/poolsync-watchdog.sh"

grep -Fx -- '-H' "$CALLS/curl-args" >/dev/null
grep -Fx -- 'Authorization: Bearer test-token' "$CALLS/curl-args" >/dev/null
grep -Fx -- 'http://hub.test:9470/api/status' "$CALLS/curl-args" >/dev/null
[[ "$(cat "$TEST_HOME/.cache/poolsync/node-misses")" == 0 ]]
[[ ! -e "$CALLS/restarts" ]]

echo "watchdog-test: authenticated healthy node is not restarted"
