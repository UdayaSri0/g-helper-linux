#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
source "$SCRIPT_DIR/package-common.sh"

OUTPUT_DIR="${1:-$REPO_ROOT/dist}"
APPDIR="${OUTPUT_DIR}/AppDir"
VERSION="$(package_version)"

mkdir -p "$OUTPUT_DIR"
generate_icon_assets
validate_desktop_entry

UI_BIN="$(ensure_release_binary rog-ui rog-helper-ui)"

rm -rf "$APPDIR"
mkdir -p "$APPDIR/usr/bin"
install -Dm0755 "$UI_BIN" "$APPDIR/usr/bin/rog-helper-ui"
install_desktop_entry "$APPDIR"
install_icon_theme_assets "$APPDIR"

install -Dm0755 "$REPO_ROOT/packaging/appimage/AppRun" "$APPDIR/AppRun"
install -Dm0644 \
  "$REPO_ROOT/packaging/desktop/icons/hicolor/256x256/apps/${ICON_NAME}.png" \
  "$APPDIR/${ICON_NAME}.png"
install -Dm0644 \
  "$REPO_ROOT/packaging/desktop/icons/hicolor/256x256/apps/${ICON_NAME}.png" \
  "$APPDIR/.DirIcon"
install -Dm0644 "$DESKTOP_FILE" "$APPDIR/${ICON_NAME}.desktop"

if command -v appimagetool >/dev/null 2>&1; then
  APPIMAGE_PATH="$OUTPUT_DIR/${ICON_NAME}-${VERSION}-$(uname -m).AppImage"
  appimagetool "$APPDIR" "$APPIMAGE_PATH" >/dev/null
  echo "built $APPIMAGE_PATH"
else
  echo "staged $APPDIR (appimagetool not installed; AppImage not produced)"
fi
