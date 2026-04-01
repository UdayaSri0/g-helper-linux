#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
source "$SCRIPT_DIR/package-common.sh"

OUTPUT_DIR="${1:-$REPO_ROOT/dist}"
ARCH="$(dpkg --print-architecture)"
VERSION="$(package_version)"
PACKAGE_NAME="rog-helper"

mkdir -p "$OUTPUT_DIR"
generate_icon_assets
validate_desktop_entry

UI_BIN="$(ensure_release_binary rog-ui rog-helper-ui)"
DAEMON_BIN="$(ensure_release_binary rog-daemon rog-helperd)"
CLI_BIN="$(ensure_release_binary rog-cli rog-helper)"
DEPENDS="$(debian_runtime_depends || printf 'libc6')"

STAGE_ROOT="$(mktemp -d)"
trap 'rm -rf "$STAGE_ROOT"' EXIT

install -Dm0755 "$UI_BIN" "$STAGE_ROOT/usr/bin/rog-helper-ui"
install -Dm0755 "$DAEMON_BIN" "$STAGE_ROOT/usr/bin/rog-helperd"
install -Dm0755 "$CLI_BIN" "$STAGE_ROOT/usr/bin/rog-helper"
install_desktop_entry "$STAGE_ROOT"
install_icon_theme_assets "$STAGE_ROOT"
install_user_service "$STAGE_ROOT"

INSTALLED_SIZE="$(du -sk "$STAGE_ROOT/usr" | cut -f1)"
mkdir -p "$STAGE_ROOT/DEBIAN"
cat >"$STAGE_ROOT/DEBIAN/control" <<EOF
Package: $PACKAGE_NAME
Version: $VERSION
Section: utils
Priority: optional
Architecture: $ARCH
Maintainer: rog-helper contributors
Installed-Size: $INSTALLED_SIZE
Depends: $DEPENDS
Description: Linux-native ASUS ROG control app
 GTK4/libadwaita UI, session daemon, diagnostics CLI, desktop entry, icons,
 and systemd user service for rog-helper.
EOF

install -Dm0755 "$REPO_ROOT/packaging/debian/postinst" "$STAGE_ROOT/DEBIAN/postinst"
install -Dm0755 "$REPO_ROOT/packaging/debian/postrm" "$STAGE_ROOT/DEBIAN/postrm"

PACKAGE_PATH="$OUTPUT_DIR/${PACKAGE_NAME}_${VERSION}_${ARCH}.deb"
dpkg-deb --build --root-owner-group "$STAGE_ROOT" "$PACKAGE_PATH" >/dev/null
echo "built $PACKAGE_PATH"
