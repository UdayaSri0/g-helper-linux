#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
TEST_ROOT="$(mktemp -d)"
CALLS="$TEST_ROOT/calls"
INSTALL_OUTPUT="$TEST_ROOT/install-output.log"
on_error() {
  local status="$1"
  local command="$2"
  echo "install-dev.sh mock test failed (exit $status): $command" >&2
  if [[ -f "$INSTALL_OUTPUT" ]]; then
    echo "--- captured installer output ---" >&2
    cat "$INSTALL_OUTPUT" >&2
  fi
  if [[ -f "$CALLS" ]]; then
    echo "--- recorded mock calls ---" >&2
    cat "$CALLS" >&2
  fi
}
trap 'on_error "$?" "$BASH_COMMAND"' ERR
trap 'find "$TEST_ROOT" -depth -delete' EXIT

FAKE_REPO="$TEST_ROOT/repo"
MOCK_BIN="$TEST_ROOT/bin"
mkdir -p "$FAKE_REPO/packaging/scripts" "$MOCK_BIN"
cp "$SCRIPT_DIR/install-dev.sh" "$FAKE_REPO/packaging/scripts/install-dev.sh"
cp "$SCRIPT_DIR/../../Cargo.toml" "$FAKE_REPO/Cargo.toml"
VERSION="$(python3 -c 'from pathlib import Path; import sys, tomllib; print(tomllib.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))["workspace"]["package"]["version"])' "$FAKE_REPO/Cargo.toml")"
export VERSION

cat >"$FAKE_REPO/packaging/scripts/build-deb.sh" <<'EOF'
#!/usr/bin/env bash
set -euo pipefail
mkdir -p "$1"
: >"$1/rog-helper_${VERSION}_amd64.deb"
printf 'build-deb <%s>\n' "$1" >>"$CALLS"
EOF
chmod 0755 "$FAKE_REPO/packaging/scripts/build-deb.sh"

make_mock() {
  local name="$1"
  shift
  {
    echo '#!/usr/bin/env bash'
    echo 'set -euo pipefail'
    printf '%s\n' "$@"
  } >"$MOCK_BIN/$name"
  chmod 0755 "$MOCK_BIN/$name"
}

make_mock id 'echo 1000'
make_mock dpkg 'echo amd64'
make_mock dpkg-deb '
case "${3:-}" in
  Package) echo rog-helper ;;
  Version) echo "$VERSION" ;;
  Architecture) echo amd64 ;;
  *) exit 2 ;;
esac'
make_mock sudo 'printf "sudo" >>"$CALLS"; printf " <%s>" "$@" >>"$CALLS"; printf "\n" >>"$CALLS"'
make_mock systemctl '
printf "systemctl" >>"$CALLS"; printf " <%s>" "$@" >>"$CALLS"; printf "\n" >>"$CALLS"
[[ "$*" == "--user is-active --quiet rog-helperd.service" ]] && exit 0
exit 0'
make_mock rog-helper '
printf "rog-helper" >>"$CALLS"; printf " <%s>" "$@" >>"$CALLS"; printf "\n" >>"$CALLS"'

: >"$CALLS"
PATH="$MOCK_BIN:/usr/bin:/bin" CALLS="$CALLS" \
  "$FAKE_REPO/packaging/scripts/install-dev.sh" >"$INSTALL_OUTPUT" 2>&1

package="$FAKE_REPO/dist/rog-helper_${VERSION}_amd64.deb"
if ! grep -Fqx "sudo <apt> <install> <$package>" "$CALLS"; then
  echo "Missing expected mock call: grep -Fqx \"sudo <apt> <install> <$package>\" \"$CALLS\"" >&2
  false
fi
[[ "$(grep -c '^sudo ' "$CALLS")" -eq 1 ]]
if ! grep -Fqx "systemctl <--user> <daemon-reload>" "$CALLS"; then
  echo "Missing expected mock call: grep -Fqx \"systemctl <--user> <daemon-reload>\" \"$CALLS\"" >&2
  false
fi
if ! grep -Fqx "systemctl <--user> <restart> <rog-helperd.service>" "$CALLS"; then
  echo "Missing expected mock call: grep -Fqx \"systemctl <--user> <restart> <rog-helperd.service>\" \"$CALLS\"" >&2
  false
fi
if ! grep -Fqx "rog-helper <privileged-status>" "$CALLS"; then
  echo "Missing expected mock call: grep -Fqx \"rog-helper <privileged-status>\" \"$CALLS\"" >&2
  false
fi
if ! grep -Fqx "rog-helper <lighting-diagnostics>" "$CALLS"; then
  echo "Missing expected mock call: grep -Fqx \"rog-helper <lighting-diagnostics>\" \"$CALLS\"" >&2
  false
fi

make_mock id 'echo 0'
if PATH="$MOCK_BIN:/usr/bin:/bin" CALLS="$CALLS" \
  "$FAKE_REPO/packaging/scripts/install-dev.sh" >/dev/null 2>&1; then
  echo "install-dev.sh accepted direct root invocation" >&2
  exit 1
fi

echo "install-dev.sh tests passed"
