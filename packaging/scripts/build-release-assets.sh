#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
source "$SCRIPT_DIR/package-common.sh"

OUTPUT_DIR="${1:-$REPO_ROOT/dist}"

mkdir -p "$OUTPUT_DIR"
prepare_release_assets

install_release_binary rog-helper-ui "$OUTPUT_DIR/rog-helper-ui"
install_release_binary rog-helperd "$OUTPUT_DIR/rog-helperd"
install_release_binary rog-helper "$OUTPUT_DIR/rog-helper"

ROG_HELPER_SKIP_PREPARE=1 "$SCRIPT_DIR/build-deb.sh" "$OUTPUT_DIR"
ROG_HELPER_SKIP_PREPARE=1 "$SCRIPT_DIR/build-tarball.sh" "$OUTPUT_DIR"

if [[ "${ROG_HELPER_BUILD_APPIMAGE:-1}" == "1" ]]; then
  ROG_HELPER_SKIP_PREPARE=1 "$SCRIPT_DIR/build-appimage.sh" "$OUTPUT_DIR"
fi

write_sha256sums "$OUTPUT_DIR"
echo "release assets ready in $OUTPUT_DIR"
