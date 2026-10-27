#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
source "$SCRIPT_DIR/appimage-gtk-plugin.sh"

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT

make_fixture() {
  cat > "$WORK_DIR/plugin" <<'EOF'
#!/usr/bin/env bash
set -e
copy_lib_tree() {
  [[ "${FAIL_COPY:-0}" != 1 ]] || return 1
  mkdir -p "$2/usr/lib/gtk-4.0"
  cp "$1/runtime-resource" "$2/usr/lib/gtk-4.0/"
}
gtk4_libdir="$GTK4_LIBDIR"
gtk4_path="$gtk4_libdir"
        copy_lib_tree "$gtk4_libdir" "$APPDIR/"
test -f "$APPDIR/usr/lib/gtk-4.0/runtime-resource"
EOF
}

run_fixture() {
  local module_dir="$1"
  local appdir="$2"
  GTK4_LIBDIR="$module_dir" APPDIR="$appdir" bash "$WORK_DIR/patched"
}

# Present optional modules are copied into the AppDir runtime tree.
mkdir -p "$WORK_DIR/modules" "$WORK_DIR/AppDir"
touch "$WORK_DIR/modules/runtime-resource"
make_fixture
patch_linuxdeploy_gtk_plugin "$WORK_DIR/plugin" "$WORK_DIR/patched"
run_fixture "$WORK_DIR/modules" "$WORK_DIR/AppDir"
test -f "$WORK_DIR/AppDir/usr/lib/gtk-4.0/runtime-resource"

# An absent optional modules directory is skipped, but a real copy failure remains fatal.
make_fixture
patch_linuxdeploy_gtk_plugin "$WORK_DIR/plugin" "$WORK_DIR/patched"
GTK4_LIBDIR="$WORK_DIR/absent" APPDIR="$WORK_DIR/AppDir" bash "$WORK_DIR/patched"
make_fixture
patch_linuxdeploy_gtk_plugin "$WORK_DIR/plugin" "$WORK_DIR/patched"
if FAIL_COPY=1 run_fixture "$WORK_DIR/modules" "$WORK_DIR/AppDir"; then
  echo "GTK module copy failure was swallowed" >&2
  exit 1
fi

# The patch must not hide a missing mandatory GTK library check in the caller.
if require_gtk4_library "$WORK_DIR/missing"; then
  echo "missing mandatory GTK library was accepted" >&2
  exit 1
fi
mkdir -p "$WORK_DIR/gtk-lib"
touch "$WORK_DIR/gtk-lib/libgtk-4.so.1"
require_gtk4_library "$WORK_DIR/gtk-lib"
