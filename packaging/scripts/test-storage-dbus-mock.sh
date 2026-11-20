#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
cd "$REPO_ROOT"
dbus-run-session --config-file="$SCRIPT_DIR/session-smoke.conf" -- bash -euo pipefail -c '
  export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS"
  export ROG_STORAGE_TEST_ISOLATED_DBUS=1
  cargo test -p rog-daemon --test storage_dbus_reject --locked -- --nocapture
'
