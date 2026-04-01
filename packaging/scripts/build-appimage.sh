#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
source "$SCRIPT_DIR/package-common.sh"

OUTPUT_DIR="${1:-$REPO_ROOT/dist}"
APPDIR="${OUTPUT_DIR}/AppDir"
VERSION="$(package_version)"
ARCH="$(release_arch)"

mkdir -p "$OUTPUT_DIR"
if [[ "${ROG_HELPER_SKIP_PREPARE:-0}" != "1" ]]; then
  prepare_release_assets
fi

rm -rf "$APPDIR"
install_release_binary rog-helper-ui "$APPDIR/usr/bin/rog-helper-ui"
install_release_binary rog-helperd "$APPDIR/usr/bin/rog-helperd"
install_release_binary rog-helper "$APPDIR/usr/bin/rog-helper"
install_desktop_entry "$APPDIR/usr"
install_icon_theme_assets "$APPDIR/usr"
install_metainfo "$APPDIR/usr"
install_dbus_activation "$APPDIR/usr" "rog-helperd"
install_user_service "$APPDIR/usr" "rog-helperd"
install_license_docs "$APPDIR/usr"

install -Dm0755 "$REPO_ROOT/packaging/appimage/AppRun" "$APPDIR/AppRun"
install -Dm0644 \
  "$REPO_ROOT/packaging/desktop/icons/hicolor/256x256/apps/${ICON_NAME}.png" \
  "$APPDIR/${ICON_NAME}.png"
install -Dm0644 \
  "$REPO_ROOT/packaging/desktop/icons/hicolor/256x256/apps/${ICON_NAME}.png" \
  "$APPDIR/.DirIcon"
install -Dm0644 "$DESKTOP_FILE" "$APPDIR/${ICON_NAME}.desktop"

normalize_tree_timestamps "$APPDIR"

if command -v appimagetool >/dev/null 2>&1; then
  APPIMAGE_PATH="$OUTPUT_DIR/${ICON_NAME}-${VERSION}-${ARCH}.AppImage"
  appimagetool "$APPDIR" "$APPIMAGE_PATH" >/dev/null
  echo "built $APPIMAGE_PATH"
else
  APPDIR_TARBALL="$OUTPUT_DIR/${ICON_NAME}-${VERSION}-${ARCH}.AppDir.tar.xz"
  tar --sort=name \
    --mtime="@$SOURCE_DATE_EPOCH" \
    --owner=0 \
    --group=0 \
    --numeric-owner \
    -C "$OUTPUT_DIR" \
    -cJf "$APPDIR_TARBALL" \
    "$(basename "$APPDIR")"
  echo "built $APPDIR_TARBALL (AppDir fallback; appimagetool not installed)"
fi
