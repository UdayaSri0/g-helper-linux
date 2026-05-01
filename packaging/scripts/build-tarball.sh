#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
source "$SCRIPT_DIR/package-common.sh"

OUTPUT_DIR="${1:-$REPO_ROOT/dist}"
VERSION="$(package_version)"
ARCH="$(release_arch)"
BUNDLE_NAME="${PACKAGE_NAME}-${VERSION}-linux-${ARCH}"

mkdir -p "$OUTPUT_DIR"
if [[ "${ROG_HELPER_SKIP_PREPARE:-0}" != "1" ]]; then
  prepare_release_assets
fi

STAGE_ROOT="$(mktemp -d)"
trap 'rm -rf "$STAGE_ROOT"' EXIT

BUNDLE_ROOT="$STAGE_ROOT/$BUNDLE_NAME"
install_release_binary rog-helper-ui "$BUNDLE_ROOT/bin/rog-helper-ui"
install_release_binary rog-helperd "$BUNDLE_ROOT/bin/rog-helperd"
install_release_binary rog-helper "$BUNDLE_ROOT/bin/rog-helper"
install_desktop_entry "$BUNDLE_ROOT"
install_icon_theme_assets "$BUNDLE_ROOT"
install_app_symbolic_icons "$BUNDLE_ROOT"
install_metainfo "$BUNDLE_ROOT"
install_dbus_activation "$BUNDLE_ROOT" "rog-helperd"
install_user_service "$BUNDLE_ROOT" "rog-helperd"
install_license_docs "$BUNDLE_ROOT"

normalize_tree_timestamps "$BUNDLE_ROOT"

TARBALL_PATH="$OUTPUT_DIR/${BUNDLE_NAME}.tar.xz"
tar --sort=name \
  --mtime="@$SOURCE_DATE_EPOCH" \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  -C "$STAGE_ROOT" \
  -cJf "$TARBALL_PATH" \
  "$BUNDLE_NAME"
echo "built $TARBALL_PATH"
