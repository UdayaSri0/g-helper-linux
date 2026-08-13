#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
source "$SCRIPT_DIR/package-common.sh"

OUTPUT_DIR="${1:-$REPO_ROOT/dist}"
VERSION="$(package_version)"
RPM_RELEASE="${ROG_HELPER_RPM_RELEASE:-1}"

require_rpm_tools() {
  local missing=()
  local cmd

  for cmd in rpmbuild rpm; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
      missing+=("$cmd")
    fi
  done

  if ((${#missing[@]} != 0)); then
    printf 'missing RPM build dependencies: %s\n' "${missing[*]}" >&2
    echo "Fedora hint: dnf install cargo rust gcc pkgconf-pkg-config gtk4-devel libadwaita-devel appstream desktop-file-utils python3-pillow rpm-build" >&2
    exit 1
  fi
}

mkdir -p "$OUTPUT_DIR"
if [[ "${ROG_HELPER_SKIP_PREPARE:-0}" != "1" ]]; then
  prepare_release_assets
fi

require_rpm_tools

RPMBUILD_ROOT="$(mktemp -d)"
PAYLOAD_ROOT="$(mktemp -d)"
trap 'rm -rf "$RPMBUILD_ROOT" "$PAYLOAD_ROOT"' EXIT

mkdir -p "$RPMBUILD_ROOT"/BUILD "$RPMBUILD_ROOT"/BUILDROOT "$RPMBUILD_ROOT"/RPMS \
  "$RPMBUILD_ROOT"/SOURCES "$RPMBUILD_ROOT"/SPECS "$RPMBUILD_ROOT"/SRPMS

install_release_binary rog-helper-ui "$PAYLOAD_ROOT/usr/bin/rog-helper-ui"
install_release_binary rog-helperd "$PAYLOAD_ROOT/usr/bin/rog-helperd"
install_release_binary rog-helper "$PAYLOAD_ROOT/usr/bin/rog-helper"
install_release_binary rog-helper-privileged "$PAYLOAD_ROOT/usr/libexec/rog-helper-privileged"
install_desktop_entry "$PAYLOAD_ROOT/usr"
install_icon_theme_assets "$PAYLOAD_ROOT/usr"
install_app_symbolic_icons "$PAYLOAD_ROOT/usr"
install_metainfo "$PAYLOAD_ROOT/usr"
install_dbus_activation "$PAYLOAD_ROOT/usr" "/usr/bin/rog-helperd"
install_user_service "$PAYLOAD_ROOT/usr" "/usr/bin/rog-helperd"
install_privileged_integration "$PAYLOAD_ROOT/usr" "/usr/libexec/rog-helper-privileged"
install_license_docs "$PAYLOAD_ROOT/usr"

normalize_tree_timestamps "$PAYLOAD_ROOT"

SOURCE_TARBALL="$RPMBUILD_ROOT/SOURCES/${PACKAGE_NAME}-${VERSION}-rpm-root.tar.xz"
tar --sort=name \
  --mtime="@$SOURCE_DATE_EPOCH" \
  --owner=0 \
  --group=0 \
  --numeric-owner \
  -C "$PAYLOAD_ROOT" \
  -cJf "$SOURCE_TARBALL" \
  usr

SPEC_PATH="$RPMBUILD_ROOT/SPECS/${PACKAGE_NAME}.spec"
write_rpm_spec "$SPEC_PATH" "$RPM_RELEASE"

rpmbuild \
  --define "_topdir $RPMBUILD_ROOT" \
  --define "_build_id_links none" \
  -bb "$SPEC_PATH" \
  >/dev/null

RPM_PATH="$(find "$RPMBUILD_ROOT/RPMS" -type f -name "${PACKAGE_NAME}-${VERSION}-${RPM_RELEASE}.*.rpm" ! -name "*.src.rpm" | head -n1)"
if [[ -z "$RPM_PATH" ]]; then
  echo "rpmbuild did not produce an RPM artifact" >&2
  exit 1
fi

FINAL_RPM_PATH="$OUTPUT_DIR/$(basename "$RPM_PATH")"
cp -f "$RPM_PATH" "$FINAL_RPM_PATH"

CHECKSUM_PATH="$OUTPUT_DIR/${PACKAGE_NAME}-${VERSION}-RPM-SHA256SUMS.txt"
(
  cd "$OUTPUT_DIR"
  sha256sum "$(basename "$FINAL_RPM_PATH")" > "$(basename "$CHECKSUM_PATH")"
)

echo "built $FINAL_RPM_PATH"
