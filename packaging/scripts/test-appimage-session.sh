#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
APPDIR="${1:?usage: test-appimage-session.sh APPDIR OUTPUT_DIR}"
OUTPUT_DIR="${2:?usage: test-appimage-session.sh APPDIR OUTPUT_DIR}"
BIN_DIR="$APPDIR/usr/bin"
UI_ENTRYPOINT="$APPDIR/AppRun"
if [[ ! -x "$UI_ENTRYPOINT" ]]; then
  echo "AppDir is missing its executable AppRun entrypoint" >&2
  exit 2
fi
for binary in rog-helper-ui rog-helperd rog-helper; do
  if [[ ! -x "$BIN_DIR/$binary" ]]; then
    echo "AppDir is missing executable $binary" >&2
    exit 2
  fi
done
if [[ ! -d "$APPDIR/usr/lib" ]]; then
  echo "AppDir has no bundled runtime libraries: $APPDIR/usr/lib" >&2
  exit 2
fi

mapfile -t library_dirs < <(find "$APPDIR/usr/lib" -type d -print)
library_path="$(IFS=:; printf '%s' "${library_dirs[*]}")"
ROG_HELPER_SMOKE_BIN_DIR="$BIN_DIR" \
ROG_HELPER_SMOKE_UI_BIN="$UI_ENTRYPOINT" \
ROG_HELPER_SMOKE_APPDIR="$(realpath "$APPDIR")" \
ROG_HELPER_SMOKE_LD_LIBRARY_PATH="$library_path" \
ROG_HELPER_SMOKE_XDG_DATA_DIRS="$APPDIR/usr/share:/usr/local/share:/usr/share" \
  xvfb-run -a "$SCRIPT_DIR/test-session-smoke.sh" "$OUTPUT_DIR"
