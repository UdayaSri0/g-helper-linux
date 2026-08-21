#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"

if [[ "$#" -ne 1 || ! -f "$1" || "$1" != *.deb ]]; then
  echo "usage: ${0##*/} PACKAGE.deb" >&2
  exit 2
fi

for command_name in \
  appstreamcli desktop-file-validate dpkg-deb python3 systemd-analyze udevadm; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "package validation requires: $command_name" >&2
    exit 2
  fi
done

python3 "$SCRIPT_DIR/validate-package-payload.py" "$1"
echo "Debian package content, metadata, permissions, and security checks passed: $1"
