#!/usr/bin/env bash
# Isolated session/system bus namespaces and fresh defaults prevent host policy/hardware actions.
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
OUTPUT_DIR="${1:-$(mktemp -d /tmp/rog-helper-session-smoke.XXXXXX)}"
mkdir -p "$OUTPUT_DIR"
SMOKE_ROOT="$(mktemp -d "$OUTPUT_DIR/session.XXXXXX")"
export ROG_HELPER_SMOKE_ROOT="$SMOKE_ROOT" ROG_HELPER_SMOKE_REPO="$REPO_ROOT"

dbus-run-session --config-file="$SCRIPT_DIR/session-smoke.conf" -- bash -euo pipefail <<'SH'
export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS"
export XDG_CONFIG_HOME="$ROG_HELPER_SMOKE_ROOT/config"
export XDG_CACHE_HOME="$ROG_HELPER_SMOKE_ROOT/cache"
export XDG_DATA_HOME="$ROG_HELPER_SMOKE_ROOT/data"
mkdir -p "$XDG_CONFIG_HOME" "$XDG_CACHE_HOME" "$XDG_DATA_HOME"
daemon="$ROG_HELPER_SMOKE_REPO/target/debug/rog-helperd"
cli="$ROG_HELPER_SMOKE_REPO/target/debug/rog-helper"
ui="$ROG_HELPER_SMOKE_REPO/target/debug/rog-helper-ui"

"$daemon" >"$ROG_HELPER_SMOKE_ROOT/daemon.log" 2>&1 &
daemon_pid=$!
trap 'kill -TERM "$daemon_pid" 2>/dev/null || true; wait "$daemon_pid" 2>/dev/null || true' EXIT
ready=0
for attempt in {1..40}; do
  if gdbus introspect --session --dest io.github.roghelper.Daemon \
    --object-path /io/github/roghelper/Daemon >"$ROG_HELPER_SMOKE_ROOT/introspection.txt" 2>/dev/null; then
    ready=1
    break
  fi
  sleep 0.2
done
[[ "$ready" -eq 1 ]]
grep -Fq 'GetProfileChoices' "$ROG_HELPER_SMOKE_ROOT/introspection.txt"
grep -Fq 'PauseAutomation' "$ROG_HELPER_SMOKE_ROOT/introspection.txt"
gdbus call --session --dest io.github.roghelper.Daemon --object-path /io/github/roghelper/Daemon \
  --method io.github.roghelper.Daemon1.GetPrivilegedStatus >"$ROG_HELPER_SMOKE_ROOT/helper-status.txt"
grep -Fq "'privileged_helper_reachable': <false>" "$ROG_HELPER_SMOKE_ROOT/helper-status.txt"
"$cli" automation pause
"$cli" automation resume
if "$cli" profile cycle >"$ROG_HELPER_SMOKE_ROOT/profile-cycle.txt" 2>&1; then
  echo "profile cycling unexpectedly succeeded without asusd" >&2
  exit 1
fi
grep -Fq 'unavailable' "$ROG_HELPER_SMOKE_ROOT/profile-cycle.txt"
"$cli" issue-report >"$ROG_HELPER_SMOKE_ROOT/issue-report.md"
grep -Fq 'Build base commit:' "$ROG_HELPER_SMOKE_ROOT/issue-report.md"
grep -Fq 'Policy engine state' "$ROG_HELPER_SMOKE_ROOT/issue-report.md"
if [[ -n "${DISPLAY:-}${WAYLAND_DISPLAY:-}" ]]; then
  ui_status=0
  timeout --signal=TERM 5 "$ui" >"$ROG_HELPER_SMOKE_ROOT/ui.log" 2>&1 || ui_status=$?
  [[ "$ui_status" -eq 124 ]] || { echo "UI exited unexpectedly ($ui_status)" >&2; exit 1; }
  echo "UI remained running for five seconds; visual/interactive behavior not asserted"
else
  echo "UI smoke skipped: no display session"
fi
kill -TERM "$daemon_pid"
wait "$daemon_pid"
trap - EXIT
SH

echo "isolated missing-helper/asusd/supergfxd session smoke passed; evidence: $SMOKE_ROOT"
