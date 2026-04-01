#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"

PACKAGE_NAME="rog-helper"
ICON_NAME="rog-helper"
DEBIAN_DIR="$REPO_ROOT/packaging/debian"
DESKTOP_FILE="$REPO_ROOT/packaging/desktop/rog-helper.desktop"
APPSTREAM_FILE="$REPO_ROOT/packaging/metainfo/io.github.roghelper.UI.metainfo.xml"
DBUS_SESSION_SERVICE="$REPO_ROOT/packaging/dbus-session/io.github.roghelper.Daemon.service"
SYSTEMD_USER_SERVICE="$REPO_ROOT/packaging/systemd-user/rog-helperd.service"
LICENSE_FILES=("$REPO_ROOT/LICENSE-APACHE" "$REPO_ROOT/LICENSE-MIT")
ICON_SIZES=(16 24 32 48 64 128 256 512)

workspace_package_field() {
  python3 - "$REPO_ROOT/Cargo.toml" "$1" <<'PY'
from pathlib import Path
import sys
import tomllib

manifest_path, field = sys.argv[1:3]
workspace = tomllib.loads(Path(manifest_path).read_text())
value = workspace.get("workspace", {}).get("package", {}).get(field)
if value is None:
    raise SystemExit(f"missing workspace.package.{field}")
if isinstance(value, list):
    print(":".join(str(item) for item in value))
else:
    print(value)
PY
}

package_version() {
  workspace_package_field version
}

package_license() {
  workspace_package_field license
}

package_repository() {
  workspace_package_field repository
}

package_summary() {
  printf '%s\n' "Linux-native ASUS ROG control app"
}

package_description_lines() {
  cat <<'EOF'
 GTK4/libadwaita UI, session daemon, diagnostics CLI, desktop entry, icons,
 AppStream metadata, session DBus activation, and a systemd user service for
 rog-helper.
EOF
}

package_maintainer() {
  python3 - "$REPO_ROOT/Cargo.toml" <<'PY'
from pathlib import Path
import sys
import tomllib

workspace = tomllib.loads(Path(sys.argv[1]).read_text())
authors = workspace.get("workspace", {}).get("package", {}).get("authors", [])
for author in authors:
    author = str(author).strip()
    if author:
        print(author)
        raise SystemExit(0)
print("rog-helper contributors")
PY
}

release_arch() {
  uname -m
}

source_date_epoch() {
  if [[ -n "${SOURCE_DATE_EPOCH:-}" ]]; then
    printf '%s\n' "$SOURCE_DATE_EPOCH"
    return 0
  fi
  if git -C "$REPO_ROOT" rev-parse HEAD >/dev/null 2>&1; then
    git -C "$REPO_ROOT" log -1 --format=%ct
    return 0
  fi
  date +%s
}

export_source_date_epoch() {
  export SOURCE_DATE_EPOCH
  SOURCE_DATE_EPOCH="$(source_date_epoch)"
}

normalize_tree_timestamps() {
  local root="$1"
  find "$root" -exec touch -h -d "@$SOURCE_DATE_EPOCH" {} +
}

render_template_file() {
  local template="$1"
  local dest="$2"
  shift 2
  python3 - "$template" "$dest" "$@" <<'PY'
from pathlib import Path
import sys

template_path = Path(sys.argv[1])
dest_path = Path(sys.argv[2])
text = template_path.read_text()

for pair in sys.argv[3:]:
    key, value = pair.split("=", 1)
    text = text.replace(f"@{key}@", value)

dest_path.parent.mkdir(parents=True, exist_ok=True)
dest_path.write_text(text)
PY
}

render_exec_path_file() {
  local src="$1"
  local dest="$2"
  local exec_path="$3"
  python3 - "$src" "$dest" "$exec_path" <<'PY'
from pathlib import Path
import sys

src, dest, exec_path = sys.argv[1:4]
text = Path(src).read_text()
text = text.replace("rog-helperd", exec_path)
Path(dest).parent.mkdir(parents=True, exist_ok=True)
Path(dest).write_text(text)
PY
}

generate_icon_assets() {
  (
    cd "$REPO_ROOT"
    python3 "$REPO_ROOT/packaging/scripts/generate_icons.py"
  )
}

validate_desktop_entry() {
  if command -v desktop-file-validate >/dev/null 2>&1; then
    desktop-file-validate "$DESKTOP_FILE"
  fi
}

validate_metainfo() {
  if command -v appstreamcli >/dev/null 2>&1; then
    appstreamcli validate --no-net "$APPSTREAM_FILE"
  fi
}

build_release_workspace() {
  (
    cd "$REPO_ROOT"
    cargo build --release --workspace --locked >/dev/null
  )
}

prepare_release_assets() {
  export_source_date_epoch
  generate_icon_assets
  validate_desktop_entry
  validate_metainfo
  build_release_workspace
}

ensure_release_binary() {
  local binary="$1"
  local path="$REPO_ROOT/target/release/$binary"
  if [[ ! -x "$path" ]]; then
    echo "missing release binary: $path" >&2
    return 1
  fi
  printf '%s\n' "$path"
}

install_release_binary() {
  local binary="$1"
  local target="$2"
  install -Dm0755 "$(ensure_release_binary "$binary")" "$target"
}

install_icon_theme_assets() {
  local prefix_root="$1"
  for size in "${ICON_SIZES[@]}"; do
    install -Dm0644 \
      "$REPO_ROOT/packaging/desktop/icons/hicolor/${size}x${size}/apps/${ICON_NAME}.png" \
      "$prefix_root/share/icons/hicolor/${size}x${size}/apps/${ICON_NAME}.png"
  done
  install -Dm0644 \
    "$REPO_ROOT/packaging/desktop/icons/hicolor/256x256/apps/${ICON_NAME}.png" \
    "$prefix_root/share/pixmaps/${ICON_NAME}.png"
}

install_desktop_entry() {
  local prefix_root="$1"
  install -Dm0644 "$DESKTOP_FILE" "$prefix_root/share/applications/${ICON_NAME}.desktop"
}

install_metainfo() {
  local prefix_root="$1"
  install -Dm0644 \
    "$APPSTREAM_FILE" \
    "$prefix_root/share/metainfo/io.github.roghelper.UI.metainfo.xml"
}

install_dbus_activation() {
  local prefix_root="$1"
  local exec_path="$2"
  render_exec_path_file \
    "$DBUS_SESSION_SERVICE" \
    "$prefix_root/share/dbus-1/services/io.github.roghelper.Daemon.service" \
    "$exec_path"
}

install_user_service() {
  local prefix_root="$1"
  local exec_path="$2"
  render_exec_path_file \
    "$SYSTEMD_USER_SERVICE" \
    "$prefix_root/lib/systemd/user/rog-helperd.service" \
    "$exec_path"
}

install_license_docs() {
  local prefix_root="$1"
  for license_file in "${LICENSE_FILES[@]}"; do
    install -Dm0644 \
      "$license_file" \
      "$prefix_root/share/doc/$PACKAGE_NAME/$(basename "$license_file")"
  done
}

install_debian_doc_files() {
  local prefix_root="$1"
  install -Dm0644 \
    "$DEBIAN_DIR/README.Debian" \
    "$prefix_root/share/doc/$PACKAGE_NAME/README.Debian"
  install -Dm0644 \
    "$DEBIAN_DIR/copyright" \
    "$prefix_root/share/doc/$PACKAGE_NAME/copyright"
}

write_debian_control() {
  local dest="$1"
  local arch="$2"
  local depends="$3"
  local installed_size="$4"
  render_template_file \
    "$DEBIAN_DIR/control.in" \
    "$dest" \
    "PACKAGE_NAME=$PACKAGE_NAME" \
    "VERSION=$(package_version)" \
    "ARCH=$arch" \
    "MAINTAINER=$(package_maintainer)" \
    "HOMEPAGE=$(package_repository)" \
    "INSTALLED_SIZE=$installed_size" \
    "DEPENDS=$depends" \
    "SUMMARY=$(package_summary)" \
    "DESCRIPTION=$(package_description_lines)"
}

debian_runtime_depends() {
  local stage_root="$1"
  local workdir
  local output
  workdir="$(mktemp -d)"

  mkdir -p "$workdir/debian"
  cat >"$workdir/debian/control" <<EOF
Source: $PACKAGE_NAME
Section: utils
Priority: optional
Maintainer: $(package_maintainer)
Standards-Version: 4.6.2
Homepage: $(package_repository)

Package: $PACKAGE_NAME
Architecture: any
Description: $(package_summary)
$(package_description_lines)
EOF

  output="$(
    cd "$workdir"
    dpkg-shlibdeps -O \
      "$stage_root/usr/bin/rog-helper-ui" \
      "$stage_root/usr/bin/rog-helperd" \
      "$stage_root/usr/bin/rog-helper" |
      sed -n 's/^shlibs:Depends=//p'
  )"
  rm -rf "$workdir"
  printf '%s\n' "$output"
}

write_sha256sums() {
  local output_dir="$1"
  local checksum_file="$output_dir/${PACKAGE_NAME}-$(package_version)-SHA256SUMS.txt"
  local tmp_file
  tmp_file="$(mktemp)"

  (
    cd "$output_dir"
    find . -maxdepth 1 -type f ! -name "$(basename "$checksum_file")" -printf '%P\0' |
      sort -z |
      xargs -0r sha256sum
  ) >"$tmp_file"

  mv "$tmp_file" "$checksum_file"
  chmod 0644 "$checksum_file"
}
