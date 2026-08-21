#!/bin/sh
set -eu

SCRIPT_DIR=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
REPO_ROOT=$(CDPATH= cd -- "$SCRIPT_DIR/../.." && pwd)
TEST_ROOT=$(mktemp -d)
trap 'rm -rf "$TEST_ROOT"' EXIT HUP INT TERM

MOCK_BIN="$TEST_ROOT/bin"
CALLS="$TEST_ROOT/calls"
mkdir -p "$MOCK_BIN"

make_mock() {
    name=$1
    cat >"$MOCK_BIN/$name" <<'EOF'
#!/bin/sh
printf '%s' "${0##*/}" >>"$CALLS"
for arg in "$@"; do
    printf ' <%s>' "$arg" >>"$CALLS"
done
printf '\n' >>"$CALLS"

if [ "${0##*/}" = systemctl ] && [ "${1:-}" = is-active ]; then
    [ "${MOCK_SERVICE_ACTIVE:-0}" = 1 ]
fi
EOF
    chmod 0755 "$MOCK_BIN/$name"
}

for command_name in systemctl udevadm update-desktop-database gtk-update-icon-cache; do
    make_mock "$command_name"
done

run_script() {
    : >"$CALLS"
    PATH="$MOCK_BIN:/usr/bin:/bin" CALLS="$CALLS" MOCK_SERVICE_ACTIVE="${MOCK_SERVICE_ACTIVE:-0}" \
        "$@"
}

assert_has() {
    pattern=$1
    if ! grep -Fqx -- "$pattern" "$CALLS"; then
        echo "missing maintainer-script call: $pattern" >&2
        sed 's/^/  /' "$CALLS" >&2
        exit 1
    fi
}

assert_lacks() {
    pattern=$1
    if grep -Fqx -- "$pattern" "$CALLS"; then
        echo "unexpected maintainer-script call: $pattern" >&2
        sed 's/^/  /' "$CALLS" >&2
        exit 1
    fi
}

# Fresh install refreshes system metadata and the hidraw rule, but D-Bus owns
# first activation of the root helper.
MOCK_SERVICE_ACTIVE=0 run_script "$REPO_ROOT/packaging/debian/postinst" configure ""
assert_has "systemctl <daemon-reload>"
assert_lacks "systemctl <try-restart> <rog-helper-privileged.service>"
assert_has "udevadm <control> <--reload-rules>"
assert_has "udevadm <trigger> <--subsystem-match=hidraw> <--action=change>"

# Upgrade/reinstall replaces an active old helper, while leaving an inactive
# D-Bus service stopped.
MOCK_SERVICE_ACTIVE=1 run_script "$REPO_ROOT/packaging/debian/postinst" configure 0.2.2
assert_has "systemctl <is-active> <--quiet> <rog-helper-privileged.service>"
assert_has "systemctl <try-restart> <rog-helper-privileged.service>"

MOCK_SERVICE_ACTIVE=0 run_script "$REPO_ROOT/packaging/debian/postinst" configure 0.3.0
assert_has "systemctl <is-active> <--quiet> <rog-helper-privileged.service>"
assert_lacks "systemctl <try-restart> <rog-helper-privileged.service>"

# Removal stops only the package's privileged helper. It never addresses a
# user manager and leaves home-directory state untouched.
run_script "$REPO_ROOT/packaging/debian/postrm" remove
assert_has "systemctl <stop> <rog-helper-privileged.service>"
assert_has "systemctl <daemon-reload>"
assert_has "udevadm <control> <--reload-rules>"
assert_has "udevadm <trigger> <--subsystem-match=hidraw> <--action=change>"
if grep -Eq -- '--user|/home/|sudoers|chmod' "$CALLS"; then
    echo "unsafe removal action found" >&2
    sed 's/^/  /' "$CALLS" >&2
    exit 1
fi

run_script "$REPO_ROOT/packaging/debian/postrm" purge
assert_has "systemctl <stop> <rog-helper-privileged.service>"
assert_has "systemctl <daemon-reload>"
assert_has "udevadm <trigger> <--subsystem-match=hidraw> <--action=change>"

# The old package's postrm participates in upgrades. It refreshes metadata but
# must leave the helper running for the new postinst to replace atomically.
run_script "$REPO_ROOT/packaging/debian/postrm" upgrade 0.3.0
assert_lacks "systemctl <stop> <rog-helper-privileged.service>"
assert_has "systemctl <daemon-reload>"
assert_has "udevadm <control> <--reload-rules>"

# Abort/unwind calls must not mutate services or device state.
run_script "$REPO_ROOT/packaging/debian/postinst" abort-upgrade 0.3.0
[ ! -s "$CALLS" ] || {
    echo "postinst abort-upgrade unexpectedly performed actions" >&2
    exit 1
}
run_script "$REPO_ROOT/packaging/debian/postrm" abort-install 0.3.0
[ ! -s "$CALLS" ] || {
    echo "postrm abort-install unexpectedly performed actions" >&2
    exit 1
}

echo "Debian maintainer-script lifecycle tests passed"
