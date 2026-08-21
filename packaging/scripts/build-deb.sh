#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
source "$SCRIPT_DIR/package-common.sh"

OUTPUT_DIR="${1:-$REPO_ROOT/dist}"
ARCH="$(dpkg --print-architecture)"
VERSION="$(package_version)"

mkdir -p "$OUTPUT_DIR"
export_source_date_epoch
if [[ "${ROG_HELPER_SKIP_PREPARE:-0}" != "1" ]]; then
  prepare_release_assets
fi

STAGE_ROOT="$(mktemp -d)"
trap 'rm -rf "$STAGE_ROOT"' EXIT

install_release_binary rog-helper-ui "$STAGE_ROOT/usr/bin/rog-helper-ui"
install_release_binary rog-helperd "$STAGE_ROOT/usr/bin/rog-helperd"
install_release_binary rog-helper "$STAGE_ROOT/usr/bin/rog-helper"
install_release_binary rog-helper-privileged "$STAGE_ROOT/usr/libexec/rog-helper-privileged"
install_desktop_entry "$STAGE_ROOT/usr"
install_icon_theme_assets "$STAGE_ROOT/usr"
install_app_symbolic_icons "$STAGE_ROOT/usr"
install_metainfo "$STAGE_ROOT/usr"
install_dbus_activation "$STAGE_ROOT/usr" "/usr/bin/rog-helperd"
install_user_service "$STAGE_ROOT/usr" "/usr/bin/rog-helperd"
install_privileged_integration "$STAGE_ROOT/usr" "/usr/libexec/rog-helper-privileged"
install_license_docs "$STAGE_ROOT/usr"
install_debian_doc_files "$STAGE_ROOT/usr"

DEPENDS="$(debian_runtime_depends "$STAGE_ROOT" || printf 'libc6')"

INSTALLED_SIZE="$(du -sk "$STAGE_ROOT/usr" | cut -f1)"
mkdir -p "$STAGE_ROOT/DEBIAN"
write_debian_control "$STAGE_ROOT/DEBIAN/control" "$ARCH" "$DEPENDS" "$INSTALLED_SIZE"

install -Dm0755 "$REPO_ROOT/packaging/debian/postinst" "$STAGE_ROOT/DEBIAN/postinst"
install -Dm0755 "$REPO_ROOT/packaging/debian/postrm" "$STAGE_ROOT/DEBIAN/postrm"

validate_packaged_payload "$STAGE_ROOT"
normalize_tree_timestamps "$STAGE_ROOT"

PACKAGE_PATH="$OUTPUT_DIR/${PACKAGE_NAME}_${VERSION}_${ARCH}.deb"
dpkg-deb --build --root-owner-group "$STAGE_ROOT" "$PACKAGE_PATH" >/dev/null
validate_packaged_payload "$PACKAGE_PATH"
echo "built $PACKAGE_PATH"
