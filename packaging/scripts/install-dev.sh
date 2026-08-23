#!/usr/bin/env bash
set -euo pipefail

umask 0022

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
OUTPUT_DIR="$REPO_ROOT/dist"

if [[ "$(id -u)" -eq 0 ]]; then
  echo "Do not run install-dev.sh as root or with sudo." >&2
  echo "Run it as your normal desktop user; only APT will request elevation." >&2
  exit 2
fi

for command_name in dpkg dpkg-deb python3 sudo; do
  if ! command -v "$command_name" >/dev/null 2>&1; then
    echo "required command is unavailable: $command_name" >&2
    exit 2
  fi
done

VERSION="$(python3 - "$REPO_ROOT/Cargo.toml" <<'PY'
import sys
import tomllib
from pathlib import Path

print(tomllib.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))["workspace"]["package"]["version"])
PY
)"
ARCH="$(dpkg --print-architecture)"
PACKAGE_PATH="$OUTPUT_DIR/rog-helper_${VERSION}_${ARCH}.deb"

echo "Building ROG Helper release binaries and Debian package..."
"$SCRIPT_DIR/build-deb.sh" "$OUTPUT_DIR"

if [[ ! -f "$PACKAGE_PATH" ]]; then
  echo "expected package was not created: $PACKAGE_PATH" >&2
  exit 1
fi
if [[ "$(dpkg-deb -f "$PACKAGE_PATH" Package)" != "rog-helper" || \
      "$(dpkg-deb -f "$PACKAGE_PATH" Version)" != "$VERSION" || \
      "$(dpkg-deb -f "$PACKAGE_PATH" Architecture)" != "$ARCH" ]]; then
  echo "built package identity does not match this workspace" >&2
  exit 1
fi

echo
echo "Ready to install: $PACKAGE_PATH"
echo "APT will install dependencies plus the UI, daemons, D-Bus, systemd, PolicyKit, and udev integration."
echo "Administrator access is required once for this APT command:"
printf '  sudo apt install %q\n' "$PACKAGE_PATH"
echo

# This is intentionally the script's only elevated command. Everything after
# it runs in the invoking desktop user's session and environment.
sudo apt install "$PACKAGE_PATH"

echo
echo "Refreshing the current user's session daemon..."
if command -v systemctl >/dev/null 2>&1 && systemctl --user daemon-reload; then
  if systemctl --user is-active --quiet rog-helperd.service; then
    systemctl --user restart rog-helperd.service
    echo "Session daemon restarted."
  else
    echo "Session daemon is inactive; D-Bus will activate it on demand."
  fi
else
  echo "User systemd manager is unavailable; session D-Bus activation remains available." >&2
fi

if ! command -v rog-helper >/dev/null 2>&1; then
  echo "installed rog-helper CLI is not available on PATH" >&2
  exit 1
fi

echo
echo "Checking installed integration..."
status=0
rog-helper privileged-status || status=1
echo
rog-helper lighting-diagnostics || status=1
echo
if [[ "$status" -eq 0 ]]; then
  echo "ROG Helper installation checks completed successfully."
else
  echo "ROG Helper was installed, but one or more diagnostics reported a problem." >&2
fi
exit "$status"
