#!/usr/bin/env bash

# Patch the pinned linuxdeploy GTK plugin after its upstream checksum is verified.
patch_linuxdeploy_gtk_plugin() {
  local source="$1"
  local destination="$2"
  local target='        copy_lib_tree "$gtk4_libdir" "$APPDIR/"'

  if [[ "$(grep -Fxc "$target" "$source")" != "1" ]]; then
    echo "unexpected pinned GTK plugin layout; refusing to patch" >&2
    return 1
  fi

  sed \
    "/^        copy_lib_tree \"\\\$gtk4_libdir\" \"\\\$APPDIR\\/\"$/c\\
        if [[ -d \"\\\$gtk4_libdir\" ]]; then\\
            copy_lib_tree \"\\\$gtk4_libdir\" \"\\\$APPDIR/\"\\
        else\\
            echo \"Skipping absent optional GTK 4 modules: \\\$gtk4_libdir\"\\
        fi" \
    "$source" > "$destination"
  chmod 0755 "$destination"
}

require_gtk4_library() {
  local libdir="$1"

  if [[ ! -e "$libdir/libgtk-4.so.1" ]]; then
    echo "missing mandatory GTK4 library: $libdir/libgtk-4.so.1" >&2
    return 1
  fi
}
