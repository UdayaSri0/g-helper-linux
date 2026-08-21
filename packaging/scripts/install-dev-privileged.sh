#!/usr/bin/env bash
set -euo pipefail

umask 0022

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd -- "$SCRIPT_DIR/../.." && pwd)"
source "$SCRIPT_DIR/package-common.sh"

for command in cargo install python3 systemctl udevadm busctl; do
  if ! command -v "$command" >/dev/null 2>&1; then
    echo "required command is unavailable: $command" >&2
    exit 1
  fi
done

if [[ "$(id -u)" -eq 0 ]]; then
  AS_ROOT=()
elif command -v sudo >/dev/null 2>&1; then
  AS_ROOT=(sudo)
else
  echo "sudo is required to install the root-owned helper files" >&2
  exit 1
fi

STAGE_ROOT="$(mktemp -d)"
trap 'rm -rf "$STAGE_ROOT"' EXIT

echo "Building the current rog-helper-privileged source..."
(
  cd "$REPO_ROOT"
  cargo build --locked -p rog-privileged --bin rog-helper-privileged
)

install -Dm0755 \
  "$REPO_ROOT/target/debug/rog-helper-privileged" \
  "$STAGE_ROOT/usr/libexec/rog-helper-privileged"
install_privileged_integration "$STAGE_ROOT/usr" "/usr/libexec/rog-helper-privileged"
python3 "$SCRIPT_DIR/validate-package-payload.py" --privileged-only "$STAGE_ROOT"

# Stop a previously activated copy before atomically replacing its executable.
"${AS_ROOT[@]}" systemctl stop rog-helper-privileged.service >/dev/null 2>&1 || true
"${AS_ROOT[@]}" install -Dm0755 \
  "$STAGE_ROOT/usr/libexec/rog-helper-privileged" \
  /usr/libexec/rog-helper-privileged
"${AS_ROOT[@]}" install -Dm0644 \
  "$STAGE_ROOT/usr/share/dbus-1/system-services/io.github.roghelper.Privileged.service" \
  /usr/share/dbus-1/system-services/io.github.roghelper.Privileged.service
"${AS_ROOT[@]}" install -Dm0644 \
  "$STAGE_ROOT/usr/share/dbus-1/system.d/io.github.roghelper.Privileged.conf" \
  /usr/share/dbus-1/system.d/io.github.roghelper.Privileged.conf
"${AS_ROOT[@]}" install -Dm0644 \
  "$STAGE_ROOT/usr/lib/systemd/system/rog-helper-privileged.service" \
  /usr/lib/systemd/system/rog-helper-privileged.service
"${AS_ROOT[@]}" install -Dm0644 \
  "$STAGE_ROOT/usr/share/polkit-1/actions/io.github.roghelper.policy" \
  /usr/share/polkit-1/actions/io.github.roghelper.policy
"${AS_ROOT[@]}" install -Dm0644 \
  "$STAGE_ROOT/usr/lib/udev/rules.d/60-rog-helper-aura.rules" \
  /usr/lib/udev/rules.d/60-rog-helper-aura.rules

"${AS_ROOT[@]}" systemctl daemon-reload
"${AS_ROOT[@]}" udevadm control --reload-rules

is_target_aura_hidraw() {
  local node="$1"
  local interface=""
  local vendor=""
  local product=""

  node="$(readlink -f "$node/device")"
  while [[ "$node" == /sys/* ]]; do
    if [[ -r "$node/bInterfaceNumber" ]]; then
      read -r interface <"$node/bInterfaceNumber"
    fi
    if [[ -r "$node/idVendor" && -r "$node/idProduct" ]]; then
      read -r vendor <"$node/idVendor"
      read -r product <"$node/idProduct"
      break
    fi
    node="${node%/*}"
  done

  [[ "$vendor" == "0b05" && "$product" == "19b6" && "$interface" == "00" ]]
}

target_count=0
for hidraw in /sys/class/hidraw/hidraw*; do
  [[ -e "$hidraw" ]] || continue
  if is_target_aura_hidraw "$hidraw"; then
    "${AS_ROOT[@]}" udevadm trigger --action=change "$hidraw"
    target_count=$((target_count + 1))
  fi
done
"${AS_ROOT[@]}" udevadm settle --timeout=10 || true

python3 "$SCRIPT_DIR/validate-package-payload.py" --privileged-only /
python3 - <<'PY'
import xml.etree.ElementTree as ET
ET.parse("/usr/share/polkit-1/actions/io.github.roghelper.policy")
PY

if [[ "$(systemctl show -p LoadState --value rog-helper-privileged.service)" != "loaded" ]]; then
  echo "systemd did not load rog-helper-privileged.service" >&2
  echo "Run: systemctl status rog-helper-privileged.service --no-pager" >&2
  exit 1
fi

if ! introspection="$(busctl --system introspect \
  io.github.roghelper.Privileged \
  /io/github/roghelper/Privileged)"; then
  echo "system D-Bus could not activate the privileged helper" >&2
  echo "Run: journalctl -u rog-helper-privileged.service -b --no-pager" >&2
  exit 1
fi
if [[ "$introspection" != *"SetAuraEffect"* || "$introspection" != *"GetCapabilities"* ]]; then
  echo "the activated helper does not expose the required Aura API" >&2
  echo "Check for a stale process: systemctl status rog-helper-privileged.service --no-pager" >&2
  exit 1
fi

if ! capabilities="$(busctl --system call \
  io.github.roghelper.Privileged \
  /io/github/roghelper/Privileged \
  io.github.roghelper.Privileged1 \
  GetCapabilities)"; then
  echo "the activated helper did not return its capability contract" >&2
  exit 1
fi
read -r capabilities_signature capabilities_api_version _ <<<"$capabilities"
if [[ "$capabilities_signature" != "uas" || "$capabilities_api_version" != "2" || "$capabilities" != *'"lighting"'* ]]; then
  echo "incompatible privileged capability response: $capabilities" >&2
  echo "Expected API 2 with the lighting category." >&2
  exit 1
fi

echo
echo "ROG Helper privileged helper installed"
echo
echo "DBus service: ready"
echo "PolicyKit: ready"
if [[ "$target_count" -gt 0 ]]; then
  echo "Aura device: detected"
  if [[ -L /dev/rog-helper-aura && -e /dev/rog-helper-aura ]]; then
    echo "Aura symlink: ready"
  else
    echo "Aura symlink: missing" >&2
    echo "Inspect: udevadm info /sys/class/hidraw/hidraw*" >&2
    exit 1
  fi
else
  echo "Aura device: not detected (no 0b05:19b6 interface 00 hidraw device)"
  echo "Aura symlink: not expected on this machine"
fi
echo
echo "Run rog-helperd and rog-helper-ui as your normal desktop user; never with sudo."
