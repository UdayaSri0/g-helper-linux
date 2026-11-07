#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
APPIMAGE="${1:?usage: test-appimage-runtime.sh APPIMAGE OUTPUT_DIR}"
OUTPUT_DIR="${2:?usage: test-appimage-runtime.sh APPIMAGE OUTPUT_DIR}"
if [[ ! -f "$APPIMAGE" || ! -x "$APPIMAGE" ]]; then
  echo "AppImage must exist and be executable: $APPIMAGE" >&2
  exit 2
fi
for command_name in dbus-run-session timeout xvfb-run; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "AppImage runtime smoke requires: $command_name" >&2
    exit 2
  fi
done

mkdir -p "$OUTPUT_DIR"
SMOKE_ROOT="$(mktemp -d /tmp/rog-helper-appimage-smoke.XXXXXX)"
LOG="$OUTPUT_DIR/appimage-runtime-smoke.log"
trap 'rm -rf "$SMOKE_ROOT"' EXIT HUP INT TERM
mkdir -p "$SMOKE_ROOT/home" "$SMOKE_ROOT/config" "$SMOKE_ROOT/cache" "$SMOKE_ROOT/data"

set +e
APPIMAGE_EXTRACT_AND_RUN=1 \
GDK_BACKEND=x11 \
HOME="$SMOKE_ROOT/home" \
XDG_CONFIG_HOME="$SMOKE_ROOT/config" \
XDG_CACHE_HOME="$SMOKE_ROOT/cache" \
XDG_DATA_HOME="$SMOKE_ROOT/data" \
ROG_HELPER_SMOKE_APPIMAGE="$(realpath "$APPIMAGE")" \
timeout --signal=TERM --kill-after=5s 15s \
  dbus-run-session --config-file="$SCRIPT_DIR/session-smoke.conf" -- \
  bash -euo pipefail -c '
    export DBUS_SYSTEM_BUS_ADDRESS="$DBUS_SESSION_BUS_ADDRESS"
    exec xvfb-run -a "$ROG_HELPER_SMOKE_APPIMAGE"
  ' >"$LOG" 2>&1
status=$?
set -e

if [[ "$status" -ne 124 ]] || ! grep -Fq "starting rog-helper-ui" "$LOG"; then
  echo "packaged UI did not remain running in the isolated session (status $status)" >&2
  tail -n 100 "$LOG" >&2 || true
  exit 1
fi

echo "AppImage UI process survived the 15-second startup smoke under Xvfb and a private D-Bus session."
echo "Window rendering and interactive behavior are not asserted."
echo "Smoke log: $LOG"
