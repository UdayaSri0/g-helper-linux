#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
ICON_NAME="rog-helper"
DESKTOP_FILE="$REPO_ROOT/packaging/desktop/rog-helper.desktop"
SYSTEMD_USER_SERVICE="$REPO_ROOT/packaging/systemd-user/rog-helperd.service"
ICON_SIZES=(16 24 32 48 64 128 256 512)

package_version() {
  python3 - <<'PY'
from pathlib import Path
import tomllib

root = Path.cwd()
workspace = tomllib.loads((root / "Cargo.toml").read_text())
ui = tomllib.loads((root / "crates/rog-ui/Cargo.toml").read_text())
version = ui["package"].get("version")
if version is None:
    version = workspace["workspace"]["package"].get("version")
if version is None:
    raise SystemExit("unable to determine package version")
print(version)
PY
}

generate_icon_assets() {
  python3 "$REPO_ROOT/packaging/scripts/generate_icons.py"
}

validate_desktop_entry() {
  if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$DESKTOP_FILE"
  fi
}

ensure_release_binary() {
  local crate="$1"
  local binary="$2"
  cargo build --release -p "$crate" >/dev/null
  local path="$REPO_ROOT/target/release/$binary"
  if [[ ! -x "$path" ]]; then
    echo "missing release binary: $path" >&2
    return 1
  fi
  printf '%s\n' "$path"
}

install_icon_theme_assets() {
  local stage_root="$1"
  for size in "${ICON_SIZES[@]}"; do
    install -Dm0644 \
      "$REPO_ROOT/packaging/desktop/icons/hicolor/${size}x${size}/apps/${ICON_NAME}.png" \
      "$stage_root/usr/share/icons/hicolor/${size}x${size}/apps/${ICON_NAME}.png"
  done
  install -Dm0644 \
    "$REPO_ROOT/packaging/desktop/icons/hicolor/256x256/apps/${ICON_NAME}.png" \
    "$stage_root/usr/share/pixmaps/${ICON_NAME}.png"
}

install_desktop_entry() {
  local stage_root="$1"
  install -Dm0644 "$DESKTOP_FILE" "$stage_root/usr/share/applications/${ICON_NAME}.desktop"
}

install_user_service() {
  local stage_root="$1"
  install -Dm0644 "$SYSTEMD_USER_SERVICE" "$stage_root/usr/lib/systemd/user/rog-helperd.service"
}

debian_runtime_depends() {
  local workdir
  workdir="$(mktemp -d)"
  trap 'rm -rf "$workdir"' RETURN

  mkdir -p "$workdir/debian"
  cat >"$workdir/debian/control" <<'EOF'
Source: rog-helper
Section: utils
Priority: optional
Maintainer: rog-helper contributors
Standards-Version: 4.6.2

Package: rog-helper
Architecture: any
Description: Linux-native ASUS ROG control app
 GTK4/libadwaita UI, session daemon, diagnostics CLI, desktop entry, icons,
 and systemd user service for rog-helper.
EOF

  (
    cd "$workdir"
    dpkg-shlibdeps -O \
      "$REPO_ROOT/target/release/rog-helper-ui" \
      "$REPO_ROOT/target/release/rog-helperd" \
      "$REPO_ROOT/target/release/rog-helper" |
      sed -n 's/^shlibs:Depends=//p'
  )
}
