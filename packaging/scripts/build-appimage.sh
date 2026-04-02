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
MODE="${1:-build}"

if [[ "$MODE" == "--check-deps" ]] || [[ "$MODE" == "--check" ]]; then
  OUTPUT_DIR="$REPO_ROOT/dist"
  APPDIR="${OUTPUT_DIR}/AppDir"
fi

append_path_dir() {
  local dir="$1"
  [[ -d "$dir" ]] || return 0
  case ":$PATH:" in
    *":$dir:"*) ;;
    *) PATH="$dir:$PATH" ;;
  esac
  export PATH
}

resolve_appimage_command() {
  local name="$1"
  local candidate

  if candidate="$(command -v "$name" 2>/dev/null)"; then
    printf '%s\n' "$candidate"
    return 0
  fi

  case "$name" in
    gdk-pixbuf-query-loaders)
      for candidate in \
        /usr/lib/*/gdk-pixbuf-2.0/gdk-pixbuf-query-loaders \
        /usr/lib/gdk-pixbuf-2.0/gdk-pixbuf-query-loaders \
        /usr/libexec/gdk-pixbuf-query-loaders; do
        if [[ -x "$candidate" ]]; then
          printf '%s\n' "$candidate"
          return 0
        fi
      done
      ;;
  esac

  return 1
}

require_appimage_build_deps() {
  local missing=()
  local cmd
  local module
  local resolved

  for cmd in curl file find glib-compile-schemas gdk-pixbuf-query-loaders mksquashfs patchelf pkg-config; do
    if ! resolved="$(resolve_appimage_command "$cmd")"; then
      missing+=("$cmd")
      continue
    fi
    append_path_dir "$(dirname -- "$resolved")"
  done

  for module in gio-2.0 gdk-pixbuf-2.0 gobject-introspection-1.0 gtk4 librsvg-2.0; do
    if ! pkg-config --exists "$module"; then
      missing+=("pkg-config:$module")
    fi
  done

  if ((${#missing[@]} != 0)); then
    echo "missing AppImage build dependencies:" >&2
    printf '  - %s\n' "${missing[@]}" >&2
    echo "Ubuntu/Debian hint: apt-get install file findutils libgdk-pixbuf-2.0-dev libgirepository1.0-dev libglib2.0-dev-bin librsvg2-dev patchelf squashfs-tools" >&2
    echo "On Ubuntu 24.04-style systems, gdk-pixbuf-query-loaders may live under /usr/lib/<triplet>/gdk-pixbuf-2.0/ instead of PATH." >&2
    exit 1
  fi
}

print_appimage_build_deps() {
  local cmd
  local module

  require_appimage_build_deps

  echo "AppImage dependency check passed."
  for cmd in curl file find glib-compile-schemas gdk-pixbuf-query-loaders mksquashfs patchelf pkg-config; do
    printf '  %s=%s\n' "$cmd" "$(resolve_appimage_command "$cmd")"
  done
  for module in gio-2.0 gdk-pixbuf-2.0 gobject-introspection-1.0 gtk4 librsvg-2.0; do
    printf '  pkg-config:%s=%s\n' "$module" "$(pkg-config --modversion "$module")"
  done
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

if [[ "$MODE" == "--check-deps" ]] || [[ "$MODE" == "--check" ]]; then
  print_appimage_build_deps
  exit 0
fi

if [[ -z "${SOURCE_DATE_EPOCH:-}" ]]; then
  export_source_date_epoch
fi

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

if [[ "${ROG_HELPER_WRITE_SHA256SUMS:-0}" == "1" ]]; then
  write_sha256sums "$OUTPUT_DIR"
fi

echo "built $APPIMAGE_PATH"
