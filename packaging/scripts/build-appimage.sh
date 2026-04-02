#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
source "$SCRIPT_DIR/package-common.sh"

OUTPUT_DIR="${1:-$REPO_ROOT/dist}"
APPDIR="${OUTPUT_DIR}/AppDir"
VERSION="$(package_version)"
ARCH="$(release_arch)"
TOOLS_DIR="${ROG_HELPER_APPIMAGE_TOOLS_DIR:-$REPO_ROOT/.cache/appimage-tools}"
LINUXDEPLOY_URL="${ROG_HELPER_LINUXDEPLOY_URL:-https://github.com/linuxdeploy/linuxdeploy/releases/download/1-alpha-20251107-1/linuxdeploy-x86_64.AppImage}"
APPIMAGE_PLUGIN_URL="${ROG_HELPER_LINUXDEPLOY_APPIMAGE_PLUGIN_URL:-https://github.com/linuxdeploy/linuxdeploy-plugin-appimage/releases/download/1-alpha-20250213-1/linuxdeploy-plugin-appimage-x86_64.AppImage}"
GTK_PLUGIN_URL="${ROG_HELPER_LINUXDEPLOY_GTK_PLUGIN_URL:-https://raw.githubusercontent.com/linuxdeploy/linuxdeploy-plugin-gtk/3b67a1d1c1b0c8268f57f2bce40fe2d33d409cea/linuxdeploy-plugin-gtk.sh}"
APPIMAGE_PATH="$OUTPUT_DIR/${PACKAGE_NAME}-v${VERSION}-${ARCH}.AppImage"

require_appimage_build_deps() {
  local missing=()
  local cmd
  local module

  for cmd in curl file find glib-compile-schemas gdk-pixbuf-query-loaders mksquashfs patchelf pkg-config; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
      missing+=("$cmd")
    fi
  done

  for module in gio-2.0 gdk-pixbuf-2.0 gobject-introspection-1.0 gtk4 librsvg-2.0; do
    if ! pkg-config --exists "$module"; then
      missing+=("pkg-config:$module")
    fi
  done

  if ((${#missing[@]} != 0)); then
    printf 'missing AppImage build dependencies: %s\n' "${missing[*]}" >&2
    echo "Ubuntu/Debian hint: apt-get install file findutils libgdk-pixbuf-2.0-dev libgirepository1.0-dev libglib2.0-dev-bin librsvg2-dev patchelf squashfs-tools" >&2
    exit 1
  fi
}

download_tool() {
  local url="$1"
  local dest="$2"
  local tmp

  mkdir -p "$(dirname -- "$dest")"
  if [[ -x "$dest" ]]; then
    return 0
  fi

  tmp="$(mktemp "$(dirname -- "$dest")/.${dest##*/}.XXXXXX")"
  curl --fail --location --retry 3 --show-error --silent "$url" -o "$tmp"
  chmod 0755 "$tmp"
  mv "$tmp" "$dest"
}

mkdir -p "$OUTPUT_DIR"
if [[ "${ROG_HELPER_SKIP_PREPARE:-0}" != "1" ]]; then
  prepare_release_assets
fi

if [[ "$ARCH" != "x86_64" ]]; then
  echo "AppImage builds are currently supported only on x86_64 (got $ARCH)" >&2
  exit 1
fi

require_appimage_build_deps

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
install -Dm0755 \
  "$REPO_ROOT/packaging/appimage/rog-helper-apprun-hook.sh" \
  "$APPDIR/apprun-hooks/rog-helper.sh"

normalize_tree_timestamps "$APPDIR"

download_tool "$LINUXDEPLOY_URL" "$TOOLS_DIR/linuxdeploy-x86_64.AppImage"
download_tool "$APPIMAGE_PLUGIN_URL" "$TOOLS_DIR/linuxdeploy-plugin-appimage-x86_64.AppImage"
download_tool "$GTK_PLUGIN_URL" "$TOOLS_DIR/linuxdeploy-plugin-gtk.sh"

rm -f "$APPIMAGE_PATH"
(
  cd "$REPO_ROOT"
  export APPIMAGE_EXTRACT_AND_RUN=1
  export DEPLOY_GTK_VERSION=4
  export LDAI_COMP="${ROG_HELPER_APPIMAGE_COMPRESSION:-xz}"
  export LDAI_OUTPUT="$APPIMAGE_PATH"
  export LDAI_VERSION="v${VERSION}"
  PATH="$TOOLS_DIR:$PATH" \
    "$TOOLS_DIR/linuxdeploy-x86_64.AppImage" \
      --appdir "$APPDIR" \
      --desktop-file "$APPDIR/usr/share/applications/${ICON_NAME}.desktop" \
      --icon-file "$APPDIR/usr/share/icons/hicolor/256x256/apps/${ICON_NAME}.png" \
      --executable "$APPDIR/usr/bin/rog-helper-ui" \
      --executable "$APPDIR/usr/bin/rog-helperd" \
      --executable "$APPDIR/usr/bin/rog-helper" \
      --plugin gtk \
      --output appimage \
      >/dev/null
)

normalize_tree_timestamps "$APPDIR"
touch -d "@$SOURCE_DATE_EPOCH" "$APPIMAGE_PATH"
echo "built $APPIMAGE_PATH"
