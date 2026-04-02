#!/usr/bin/env bash
set -euo pipefail

export PATH="$APPDIR/usr/bin:${PATH:-/usr/local/bin:/usr/bin}"
export XDG_DATA_DIRS="$APPDIR/usr/share:${XDG_DATA_DIRS:-/usr/local/share:/usr/share}"

if command -v dbus-update-activation-environment >/dev/null 2>&1; then
  dbus-update-activation-environment --systemd PATH XDG_DATA_DIRS >/dev/null 2>&1 || true
fi
