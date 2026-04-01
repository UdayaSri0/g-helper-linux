#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "$0")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
source "$SCRIPT_DIR/package-common.sh"

DEB_INPUT_DIR="${1:-$REPO_ROOT/dist}"
OUTPUT_DIR="${2:-$REPO_ROOT/dist/apt-repo}"
SUITE="${APT_SUITE:-stable}"
COMPONENT="${APT_COMPONENT:-main}"
ARCH="${APT_ARCH:-$(dpkg --print-architecture)}"
ORIGIN="${APT_ORIGIN:-rog-helper preview}"
LABEL="${APT_LABEL:-rog-helper preview}"
DESCRIPTION="${APT_DESCRIPTION:-Unsigned preview APT repository for rog-helper}"
POOL_LETTER="${PACKAGE_NAME:0:1}"
POOL_DIR="$OUTPUT_DIR/pool/$COMPONENT/$POOL_LETTER/$PACKAGE_NAME"
PACKAGES_DIR="$OUTPUT_DIR/dists/$SUITE/$COMPONENT/binary-$ARCH"
RELEASE_FILE="$OUTPUT_DIR/dists/$SUITE/Release"

export_source_date_epoch

mapfile -t DEB_FILES < <(
  find "$DEB_INPUT_DIR" -maxdepth 1 -type f -name "${PACKAGE_NAME}_*_${ARCH}.deb" | sort
)

if [[ "${#DEB_FILES[@]}" -eq 0 ]]; then
  echo "no ${ARCH} Debian packages found in $DEB_INPUT_DIR" >&2
  exit 1
fi

rm -rf "$OUTPUT_DIR"
mkdir -p "$POOL_DIR" "$PACKAGES_DIR"

for deb in "${DEB_FILES[@]}"; do
  install -Dm0644 "$deb" "$POOL_DIR/$(basename "$deb")"
done

(
  cd "$OUTPUT_DIR"
  dpkg-scanpackages --multiversion "pool/$COMPONENT/$POOL_LETTER/$PACKAGE_NAME" /dev/null \
    >"dists/$SUITE/$COMPONENT/binary-$ARCH/Packages"
)
gzip -9nc "$PACKAGES_DIR/Packages" >"$PACKAGES_DIR/Packages.gz"

python3 - "$OUTPUT_DIR" "$SUITE" "$COMPONENT" "$ARCH" "$ORIGIN" "$LABEL" "$DESCRIPTION" "$SOURCE_DATE_EPOCH" <<'PY'
from __future__ import annotations

from datetime import datetime, timezone
from hashlib import md5, sha256
from pathlib import Path
import sys

repo_root = Path(sys.argv[1])
suite = sys.argv[2]
component = sys.argv[3]
arch = sys.argv[4]
origin = sys.argv[5]
label = sys.argv[6]
description = sys.argv[7]
epoch = int(sys.argv[8])

suite_dir = repo_root / "dists" / suite
files = [
    suite_dir / component / f"binary-{arch}" / "Packages",
    suite_dir / component / f"binary-{arch}" / "Packages.gz",
]

lines = [
    f"Origin: {origin}",
    f"Label: {label}",
    f"Suite: {suite}",
    f"Codename: {suite}",
    f"Date: {datetime.fromtimestamp(epoch, tz=timezone.utc):%a, %d %b %Y %H:%M:%S +0000}",
    f"Architectures: {arch}",
    f"Components: {component}",
    f"Description: {description}",
    "MD5Sum:",
]

for path in files:
    rel = path.relative_to(suite_dir).as_posix()
    data = path.read_bytes()
    lines.append(f" {md5(data).hexdigest()} {path.stat().st_size:16d} {rel}")

lines.append("SHA256:")
for path in files:
    rel = path.relative_to(suite_dir).as_posix()
    data = path.read_bytes()
    lines.append(f" {sha256(data).hexdigest()} {path.stat().st_size:16d} {rel}")

(suite_dir / "Release").write_text("\n".join(lines) + "\n")
PY

cat >"$OUTPUT_DIR/README.txt" <<EOF
rog-helper APT repository preview
================================

This directory is a future-ready unsigned APT repository preview for Debian,
Ubuntu, and Linux Mint.

It is not a live install source yet.

Before publishing it as a real APT repository, you still need to:

1. host this directory on static storage such as GitHub Pages
2. sign dists/$SUITE/Release and publish InRelease/Release.gpg
3. publish the final repository URL and signed-by instructions

Generated from:
  $(printf '%s\n' "${DEB_FILES[@]##*/}")
EOF

normalize_tree_timestamps "$OUTPUT_DIR"

echo "staged APT repository preview in $OUTPUT_DIR"
