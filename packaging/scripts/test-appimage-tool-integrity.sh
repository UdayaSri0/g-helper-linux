#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
source "$SCRIPT_DIR/appimage-tool-integrity.sh"

WORK_DIR="$(mktemp -d)"
trap 'rm -rf "$WORK_DIR"' EXIT
mkdir -p "$WORK_DIR/bin" "$WORK_DIR/cache"
REAL_CURL="$(command -v curl)"
printf '#!/usr/bin/env bash\nprintf called >> %q\nexec %q "$@"\n' \
  "$WORK_DIR/curl-called" "$REAL_CURL" > "$WORK_DIR/bin/curl"
chmod 0755 "$WORK_DIR/bin/curl"
export PATH="$WORK_DIR/bin:$PATH"
printf 'trusted fixture\n' > "$WORK_DIR/expected"
EXPECTED_SHA256="$(sha256sum "$WORK_DIR/expected" | awk '{print $1}')"

# A matching cached file is reused without a network request.
cp "$WORK_DIR/expected" "$WORK_DIR/cache/tool"
chmod 0755 "$WORK_DIR/cache/tool"
download_verified_tool https://example.invalid/tool "$WORK_DIR/cache/tool" "$EXPECTED_SHA256"
test ! -e "$WORK_DIR/curl-called"

# A tampered cache fails closed without replacing it or downloading a fallback.
printf 'tampered\n' > "$WORK_DIR/cache/tool"
if download_verified_tool https://example.invalid/tool "$WORK_DIR/cache/tool" "$EXPECTED_SHA256"; then
  echo "tampered cache was accepted" >&2
  exit 1
fi
grep -q '^tampered$' "$WORK_DIR/cache/tool"
test ! -e "$WORK_DIR/curl-called"

# A matching but non-executable cache is refused without changing its mode.
cp "$WORK_DIR/expected" "$WORK_DIR/cache/non-executable"
chmod 0644 "$WORK_DIR/cache/non-executable"
if download_verified_tool https://example.invalid/tool "$WORK_DIR/cache/non-executable" "$EXPECTED_SHA256"; then
  echo "non-executable cache was accepted" >&2
  exit 1
fi
test "$(stat -c '%a' "$WORK_DIR/cache/non-executable")" = 644
test ! -e "$WORK_DIR/curl-called"

# A cache symlink is rejected without changing the external target.
cp "$WORK_DIR/expected" "$WORK_DIR/target"
chmod 0644 "$WORK_DIR/target"
ln -s "$WORK_DIR/target" "$WORK_DIR/cache/link"
if download_verified_tool https://example.invalid/tool "$WORK_DIR/cache/link" "$EXPECTED_SHA256"; then
  echo "symlink cache was accepted" >&2
  exit 1
fi
test "$(stat -c '%a' "$WORK_DIR/target")" = 644
test ! -e "$WORK_DIR/curl-called"

# A correct download is atomically cached; a mismatched download leaves no destination.
rm "$WORK_DIR/cache/tool"
download_verified_tool "file://$WORK_DIR/expected" "$WORK_DIR/cache/tool" "$EXPECTED_SHA256"
cmp "$WORK_DIR/expected" "$WORK_DIR/cache/tool"
if download_verified_tool "file://$WORK_DIR/cache/tool" "$WORK_DIR/cache/bad" "$(printf '%064d' 0)"; then
  echo "checksum-mismatched download was accepted" >&2
  exit 1
fi
test ! -e "$WORK_DIR/cache/bad"

if download_verified_tool https://example.invalid/tool "$WORK_DIR/cache/unpinned" ""; then
  echo "un-pinned tool was accepted" >&2
  exit 1
fi

echo "AppImage tool integrity tests passed"
