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
LINUXDEPLOY_STDOUT_LOG="$OUTPUT_DIR/linuxdeploy.stdout.log"
LINUXDEPLOY_STDERR_LOG="$OUTPUT_DIR/linuxdeploy.stderr.log"
LINUXDEPLOY_PLUGINS_LOG="$OUTPUT_DIR/linuxdeploy.plugins.log"
APPIMAGE_DEBUG="${ROG_HELPER_APPIMAGE_DEBUG:-}"
APPIMAGE_OUTPUT_VERBOSE="${ROG_HELPER_APPIMAGE_VERBOSE:-}"

if [[ "$MODE" == "--check-deps" ]] || [[ "$MODE" == "--check" ]]; then
  OUTPUT_DIR="$REPO_ROOT/dist"
  APPDIR="${OUTPUT_DIR}/AppDir"
fi

if [[ -z "$APPIMAGE_DEBUG" ]] && [[ -n "${CI:-}" ]]; then
  APPIMAGE_DEBUG="1"
fi
if [[ -z "$APPIMAGE_OUTPUT_VERBOSE" ]] && [[ -n "${CI:-}" ]]; then
  APPIMAGE_OUTPUT_VERBOSE="1"
fi

log_section() {
  printf '\n==> %s\n' "$1"
}

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

resolve_optional_appimage_command() {
  local name="$1"
  local candidate

  if candidate="$(command -v "$name" 2>/dev/null)"; then
    printf '%s\n' "$candidate"
    return 0
  fi

  case "$name" in
    appimagetool)
      for candidate in \
        "$TOOLS_DIR/appimagetool-x86_64.AppImage" \
        /usr/bin/appimagetool \
        /usr/local/bin/appimagetool; do
        if [[ -x "$candidate" ]]; then
          printf '%s\n' "$candidate"
          return 0
        fi
      done
      ;;
    gtk4-query-immodules)
      for candidate in \
        /usr/lib/*/gtk-4.0/gtk4-query-immodules \
        /usr/lib/gtk-4.0/gtk4-query-immodules \
        /usr/libexec/gtk4-query-immodules; do
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

supported_mksquashfs_compressors() {
  mksquashfs -help 2>&1 | awk '
    /Compressors available:/ {capture=1; next}
    capture && /^[[:space:]]+[[:alnum:]][[:alnum:]-]*([[:space:]]+\(.*\))?[[:space:]]*$/ {
      gsub(/^[[:space:]]+/, "", $0)
      print $1
      next
    }
    capture && $0 !~ /^[[:space:]]/ {exit}
  '
}

choose_appimage_compression() {
  local requested="${ROG_HELPER_APPIMAGE_COMPRESSION:-xz}"
  local resolved=""
  local supported=()
  local compressor
  local explicit=0

  if [[ -n "${ROG_HELPER_APPIMAGE_COMPRESSION:-}" ]]; then
    explicit=1
  fi

  while IFS= read -r compressor; do
    [[ -n "$compressor" ]] || continue
    supported+=("$compressor")
    if [[ "$compressor" == "$requested" ]]; then
      resolved="$requested"
    fi
  done < <(supported_mksquashfs_compressors)

  if [[ -n "$resolved" ]]; then
    printf '%s\n' "$resolved"
    return 0
  fi

  if (( explicit )); then
    echo "requested AppImage compression is not supported by mksquashfs: $requested" >&2
    printf 'supported compressors: %s\n' "${supported[*]:-none detected}" >&2
    exit 1
  fi

  for compressor in zstd gzip lz4 lzo xz lzma; do
    for resolved in "${supported[@]}"; do
      if [[ "$resolved" == "$compressor" ]]; then
        echo "default AppImage compression '$requested' is unavailable; falling back to '$compressor'" >&2
        printf '%s\n' "$compressor"
        return 0
      fi
    done
  done

  echo "could not find a supported mksquashfs compressor" >&2
  printf 'supported compressors: %s\n' "${supported[*]:-none detected}" >&2
  exit 1
}

print_tool_status() {
  local label="$1"
  local resolved="${2:-}"

  if [[ -n "$resolved" ]]; then
    printf '  %-28s %s\n' "$label" "$resolved"
  else
    printf '  %-28s %s\n' "$label" "not found"
  fi
}

print_file_status() {
  local label="$1"
  local path="$2"

  if [[ -e "$path" ]]; then
    printf '  %-28s %s\n' "$label" "$path"
    ls -ld "$path"
  else
    printf '  %-28s missing: %s\n' "$label" "$path"
  fi
}

print_dir_listing() {
  local label="$1"
  local path="$2"
  local depth="${3:-2}"

  echo "$label ($path)"
  if [[ ! -d "$path" ]]; then
    echo "  - missing"
    return 0
  fi

  find "$path" -maxdepth "$depth" -mindepth 1 | LC_ALL=C sort | sed 's#^#  - #'
}

print_appdir_summary() {
  log_section "AppDir Summary"
  print_file_status "AppDir root" "$APPDIR"
  print_file_status "UI binary" "$APPDIR/usr/bin/rog-helper-ui"
  print_file_status "daemon binary" "$APPDIR/usr/bin/rog-helperd"
  print_file_status "CLI binary" "$APPDIR/usr/bin/rog-helper"
  print_file_status "desktop file" "$APPDIR/usr/share/applications/${ICON_NAME}.desktop"
  print_file_status "metainfo" "$APPDIR/usr/share/metainfo/io.github.roghelper.UI.metainfo.xml"
  print_file_status "icon 256x256" "$APPDIR/usr/share/icons/hicolor/256x256/apps/${ICON_NAME}.png"
  print_file_status "AppRun hook" "$APPDIR/apprun-hooks/rog-helper.sh"
  print_dir_listing "AppDir root entries" "$APPDIR" 2
  print_dir_listing "usr/bin contents" "$APPDIR/usr/bin" 1
  print_dir_listing "usr/share/applications contents" "$APPDIR/usr/share/applications" 1
  print_dir_listing "usr/share/icons contents" "$APPDIR/usr/share/icons" 4
  print_dir_listing "usr/share/metainfo contents" "$APPDIR/usr/share/metainfo" 1
}

print_linuxdeploy_tooling() {
  local linuxdeploy_path="$TOOLS_DIR/linuxdeploy-x86_64.AppImage"
  local appimage_plugin_path="$TOOLS_DIR/linuxdeploy-plugin-appimage-x86_64.AppImage"
  local gtk_plugin_path="$TOOLS_DIR/linuxdeploy-plugin-gtk.sh"
  local resolved
  local compressors

  log_section "AppImage Tooling"
  print_tool_status "linuxdeploy" "$linuxdeploy_path"
  APPIMAGE_EXTRACT_AND_RUN=1 "$linuxdeploy_path" --version 2>&1 | sed 's#^#  #'
  print_tool_status "plugin-appimage" "$appimage_plugin_path"
  APPIMAGE_EXTRACT_AND_RUN=1 "$appimage_plugin_path" --plugin-api-version 2>&1 | sed 's#^#  plugin-api=#'
  print_tool_status "plugin-gtk" "$gtk_plugin_path"
  "$gtk_plugin_path" --plugin-api-version 2>&1 | sed 's#^#  plugin-api=#'

  resolved="$(resolve_appimage_command patchelf 2>/dev/null || true)"
  print_tool_status "patchelf" "$resolved"
  if [[ -n "$resolved" ]]; then
    "$resolved" --version 2>&1 | head -n 1 | sed 's#^#  #'
  fi

  resolved="$(resolve_appimage_command mksquashfs 2>/dev/null || true)"
  print_tool_status "mksquashfs" "$resolved"
  if [[ -n "$resolved" ]]; then
    "$resolved" -version 2>&1 | head -n 1 | sed 's#^#  #'
    compressors="$(supported_mksquashfs_compressors | tr '\n' ' ' | sed 's/[[:space:]]*$//')"
    printf '  %-28s %s\n' "mksquashfs compressors" "${compressors:-unknown}"
  fi

  resolved="$(resolve_appimage_command file 2>/dev/null || true)"
  print_tool_status "file" "$resolved"
  if [[ -n "$resolved" ]]; then
    "$resolved" --version 2>&1 | head -n 1 | sed 's#^#  #'
  fi

  resolved="$(resolve_appimage_command gdk-pixbuf-query-loaders 2>/dev/null || true)"
  print_tool_status "gdk-pixbuf-query-loaders" "$resolved"

  resolved="$(resolve_optional_appimage_command gtk4-query-immodules 2>/dev/null || true)"
  print_tool_status "gtk4-query-immodules" "$resolved"
  if [[ -z "$resolved" ]]; then
    echo "  gtk4-query-immodules is optional for the current linuxdeploy GTK plugin."
  fi

  resolved="$(resolve_appimage_command pkg-config 2>/dev/null || true)"
  print_tool_status "pkg-config" "$resolved"
  if [[ -n "$resolved" ]]; then
    "$resolved" --version 2>&1 | head -n 1 | sed 's#^#  #'
  fi

  resolved="$(resolve_optional_appimage_command appimagetool 2>/dev/null || true)"
  print_tool_status "appimagetool" "$resolved"
  if [[ -z "$resolved" ]]; then
    echo "  appimagetool is not called directly; linuxdeploy-plugin-appimage manages it."
  fi

  APPIMAGE_EXTRACT_AND_RUN=1 PATH="$TOOLS_DIR:$PATH" "$linuxdeploy_path" --list-plugins \
    2>&1 | tee "$LINUXDEPLOY_PLUGINS_LOG"
}

print_failure_logs() {
  local log

  for log in "$LINUXDEPLOY_PLUGINS_LOG" "$LINUXDEPLOY_STDOUT_LOG" "$LINUXDEPLOY_STDERR_LOG"; do
    if [[ -f "$log" ]]; then
      log_section "Tail of $(basename "$log")"
      tail -n 200 "$log" || true
    fi
  done
}

on_appimage_exit() {
  local exit_code="$1"

  if [[ "$exit_code" -eq 0 ]]; then
    return 0
  fi

  log_section "AppImage Build Failed"
  echo "build-appimage.sh exited with code $exit_code" >&2
  print_appdir_summary || true
  print_failure_logs || true
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

trap 'exit_code=$?; trap - EXIT; on_appimage_exit "$exit_code"; exit "$exit_code"' EXIT

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

APPIMAGE_COMPRESSION="$(choose_appimage_compression)"
export APPIMAGE_COMPRESSION
log_section "AppImage Compression"
echo "using mksquashfs compressor: $APPIMAGE_COMPRESSION"

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
ln -sf "linuxdeploy-plugin-appimage-x86_64.AppImage" "$TOOLS_DIR/linuxdeploy-plugin-appimage"
ln -sf "linuxdeploy-plugin-gtk.sh" "$TOOLS_DIR/linuxdeploy-plugin-gtk"

rm -f "$LINUXDEPLOY_STDOUT_LOG" "$LINUXDEPLOY_STDERR_LOG" "$LINUXDEPLOY_PLUGINS_LOG"
print_linuxdeploy_tooling
print_appdir_summary

rm -f "$APPIMAGE_PATH"
linuxdeploy_cmd=(
  "$TOOLS_DIR/linuxdeploy-x86_64.AppImage"
  --verbosity=0
  --appdir "$APPDIR"
  --desktop-file "$APPDIR/usr/share/applications/${ICON_NAME}.desktop"
  --icon-file "$APPDIR/usr/share/icons/hicolor/256x256/apps/${ICON_NAME}.png"
  --executable "$APPDIR/usr/bin/rog-helper-ui"
  --executable "$APPDIR/usr/bin/rog-helperd"
  --executable "$APPDIR/usr/bin/rog-helper"
  --plugin gtk
  --output appimage
)
log_section "linuxdeploy Command"
printf '  env'
printf ' %q' \
  "APPIMAGE_EXTRACT_AND_RUN=1" \
  "ARCH=$ARCH" \
  "DEPLOY_GTK_VERSION=4" \
  "LDAI_COMP=$APPIMAGE_COMPRESSION" \
  "LDAI_OUTPUT=$APPIMAGE_PATH" \
  "LDAI_VERSION=v${VERSION}" \
  "PATH=$TOOLS_DIR:$PATH"
if [[ -n "$APPIMAGE_DEBUG" ]]; then
  printf ' %q' "DEBUG=$APPIMAGE_DEBUG"
fi
if [[ -n "$APPIMAGE_OUTPUT_VERBOSE" ]]; then
  printf ' %q' "LDAI_VERBOSE=$APPIMAGE_OUTPUT_VERBOSE"
fi
printf '\n'
printf '  '
printf '%q ' "${linuxdeploy_cmd[@]}"
printf '\n'
(
  cd "$REPO_ROOT"
  export APPIMAGE_EXTRACT_AND_RUN=1
  export ARCH
  export DEPLOY_GTK_VERSION=4
  export LDAI_COMP="$APPIMAGE_COMPRESSION"
  export LDAI_OUTPUT="$APPIMAGE_PATH"
  export LDAI_VERSION="v${VERSION}"
  if [[ -n "$APPIMAGE_DEBUG" ]]; then
    export DEBUG="$APPIMAGE_DEBUG"
  fi
  if [[ -n "$APPIMAGE_OUTPUT_VERBOSE" ]]; then
    export LDAI_VERBOSE="$APPIMAGE_OUTPUT_VERBOSE"
  fi
  PATH="$TOOLS_DIR:$PATH" \
    "${linuxdeploy_cmd[@]}" \
      > >(tee "$LINUXDEPLOY_STDOUT_LOG") \
      2> >(tee "$LINUXDEPLOY_STDERR_LOG" >&2)
)

normalize_tree_timestamps "$APPDIR"
touch -d "@$SOURCE_DATE_EPOCH" "$APPIMAGE_PATH"

if [[ "${ROG_HELPER_WRITE_SHA256SUMS:-0}" == "1" ]]; then
  write_sha256sums "$OUTPUT_DIR"
fi

echo "built $APPIMAGE_PATH"
