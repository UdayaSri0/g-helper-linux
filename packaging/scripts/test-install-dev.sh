#!/usr/bin/env bash
set -euo pipefail

SCRIPT_DIR="$(cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
TEST_ROOT="$(mktemp -d)"
trap 'find "$TEST_ROOT" -depth -delete' EXIT HUP INT TERM

FAKE_REPO="$TEST_ROOT/repo"
MOCK_BIN="$TEST_ROOT/bin"
CALLS="$TEST_ROOT/calls"
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
  "$FAKE_REPO/packaging/scripts/install-dev.sh" >/dev/null

package="$FAKE_REPO/dist/rog-helper_${VERSION}_amd64.deb"
grep -Fqx "sudo <apt> <install> <$package>" "$CALLS"
[[ "$(grep -c '^sudo ' "$CALLS")" -eq 1 ]]
grep -Fqx "systemctl <--user> <daemon-reload>" "$CALLS"
grep -Fqx "systemctl <--user> <restart> <rog-helperd.service>" "$CALLS"
grep -Fqx "rog-helper <privileged-status>" "$CALLS"
grep -Fqx "rog-helper <lighting-diagnostics>" "$CALLS"

make_mock id 'echo 0'
if PATH="$MOCK_BIN:/usr/bin:/bin" CALLS="$CALLS" \
  "$FAKE_REPO/packaging/scripts/install-dev.sh" >/dev/null 2>&1; then
  echo "install-dev.sh accepted direct root invocation" >&2
  exit 1
fi

echo "install-dev.sh tests passed"
