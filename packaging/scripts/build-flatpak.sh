#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
MANIFEST_PATH="$REPO_ROOT/packaging/flatpak/io.github.roghelper.UI.yml"
BUILD_DIR="${1:-$REPO_ROOT/dist/flatpak-builder}"
REPO_DIR="${2:-$REPO_ROOT/dist/flatpak-repo}"

require_flatpak_tools() {
  local missing=()
  local cmd

  for cmd in flatpak flatpak-builder jq; do
    if ! command -v "$cmd" >/dev/null 2>&1; then
      missing+=("$cmd")
    fi
  done

  if ((${#missing[@]} != 0)); then
    printf 'missing Flatpak build dependencies: %s\n' "${missing[*]}" >&2
    echo "Ubuntu/Debian hint: apt-get install flatpak flatpak-builder jq" >&2
    exit 1
  fi
}

ensure_flathub_remote() {
  if ! flatpak remotes --columns=name 2>/dev/null | grep -qx "flathub"; then
    echo "Flathub remote not found." >&2
    echo "Run: flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo" >&2
    exit 1
  fi
}

if [[ "${1:-}" == "--check" ]]; then
  require_flatpak_tools
  jq empty "$REPO_ROOT/packaging/flatpak/cargo-sources.json" >/dev/null
  jq empty "$REPO_ROOT/packaging/flatpak/flathub.json" >/dev/null
  flatpak-builder --show-manifest "$MANIFEST_PATH" >/dev/null
  echo "validated $MANIFEST_PATH"
  exit 0
fi

require_flatpak_tools
ensure_flathub_remote

INSTALL_ARGS=()
if [[ "${ROG_HELPER_FLATPAK_INSTALL:-0}" == "1" ]]; then
  INSTALL_ARGS+=(--install)
fi

mkdir -p "$(dirname -- "$BUILD_DIR")" "$REPO_DIR"

flatpak-builder \
  --user \
  --force-clean \
  --install-deps-from=flathub \
  --repo="$REPO_DIR" \
  "${INSTALL_ARGS[@]}" \
  "$BUILD_DIR" \
  "$MANIFEST_PATH"

echo "built Flatpak repo at $REPO_DIR"
echo "install with: flatpak install --user --reinstall $REPO_DIR io.github.roghelper.UI"
