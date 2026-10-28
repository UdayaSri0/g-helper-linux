#!/usr/bin/env python3
"""Run real dpkg install, upgrade, remove, and purge transactions in a temp root."""

from __future__ import annotations

import argparse
import os
from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile
import tomllib


REQUIRED_INTEGRATION = (
    "usr/libexec/rog-helper-privileged",
    "usr/lib/systemd/system/rog-helper-privileged.service",
    "usr/lib/systemd/user/rog-helperd.service",
    "usr/lib/udev/rules.d/60-rog-helper-aura.rules",
    "usr/share/dbus-1/services/io.github.roghelper.Daemon.service",
    "usr/share/dbus-1/system-services/io.github.roghelper.Privileged.service",
    "usr/share/dbus-1/system.d/io.github.roghelper.Privileged.conf",
    "usr/share/polkit-1/actions/io.github.roghelper.policy",
)


class TestFailure(RuntimeError):
    pass


def current_deb_path() -> Path:
    root = Path(__file__).resolve().parents[2]
    cargo = tomllib.loads((root / "Cargo.toml").read_text(encoding="utf-8"))
    version = cargo["workspace"]["package"]["version"]
    return root / "dist" / f"rog-helper_{version}_amd64.deb"


def run(*args: str) -> str:
    result = subprocess.run(args, text=True, capture_output=True)
    if result.returncode:
        raise TestFailure(
            f"command failed ({result.returncode}): {' '.join(args)}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result.stdout


def copy_shell_runtime(root: Path) -> None:
    shell = Path(shutil.which("sh") or "/bin/sh").resolve()
    dependencies = subprocess.run(
        ["ldd", str(shell)], text=True, capture_output=True, check=True
    ).stdout
    paths = {shell, *(Path(path) for path in re.findall(r"(?:=>\s+)?(/[^\s]+)", dependencies))}
    if "not found" in dependencies:
        raise TestFailure(f"shell runtime has unresolved libraries:\n{dependencies}")
    for source in paths:
        if not source.is_file():
            continue
        target = root / source.relative_to("/")
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)
    shell_path = root / "bin/sh"
    shell_path.parent.mkdir(parents=True, exist_ok=True)
    shutil.copy2(shell, shell_path)
    shell_path.chmod(0o755)


def make_command_stubs(root: Path) -> Path:
    log = root / "var/tmp/rog-helper-dpkg-lifecycle.log"
    log.parent.mkdir(parents=True, exist_ok=True)
    bin_dir = root / "usr/bin"
    bin_dir.mkdir(parents=True, exist_ok=True)
    stub = """#!/bin/sh
printf '%s' "${0##*/}" >>/var/tmp/rog-helper-dpkg-lifecycle.log
for arg in "$@"; do printf ' <%s>' "$arg" >>/var/tmp/rog-helper-dpkg-lifecycle.log; done
printf '\\n' >>/var/tmp/rog-helper-dpkg-lifecycle.log
if [ "${0##*/}" = systemctl ] && [ "${1:-}" = is-active ]; then
    [ -f /var/tmp/rog-helper-service-active ]
fi
"""
    for name in (
        "systemctl",
        "udevadm",
        "update-desktop-database",
        "gtk-update-icon-cache",
    ):
        path = bin_dir / name
        path.write_text(stub, encoding="utf-8")
        path.chmod(0o755)
    return log


def initialize_root(root: Path) -> None:
    for directory in ("etc", "home", "tmp", "var/lib/dpkg", "var/tmp"):
        (root / directory).mkdir(parents=True, exist_ok=True)
    (root / "var/lib/dpkg/status").touch()
    copy_shell_runtime(root)
    make_command_stubs(root)
    # Maintainer scripts redirect optional command output here; a regular file
    # avoids creating a device node while keeping all writes inside the root.
    (root / "dev").mkdir(exist_ok=True)
    (root / "dev/null").touch()


def package_files(deb: Path, work: Path) -> tuple[Path, set[str], set[str]]:
    work.mkdir(parents=True, exist_ok=True)
    tree = work / "payload"
    control = work / "control"
    tree.mkdir()
    control.mkdir()
    run("dpkg-deb", "--extract", str(deb), str(tree))
    run("dpkg-deb", "--control", str(deb), str(control))
    files = {
        path.relative_to(tree).as_posix()
        for path in tree.rglob("*")
        if path.is_file() or path.is_symlink()
    }
    conffiles_path = control / "conffiles"
    conffiles = (
        {
            line.strip().split()[0].lstrip("/")
            for line in conffiles_path.read_text(encoding="utf-8").splitlines()
            if line.strip() and not line.lstrip().startswith("#")
        }
        if conffiles_path.exists()
        else set()
    )
    return tree, files, conffiles


def seed_unowned_state(root: Path) -> dict[str, str]:
    sentinels = {
        "home/test/.config/rog-helper/settings.toml": "user_setting=true\n",
        "usr/share/doc/rog-helper/local-administrator-note.txt": "keep me\n",
        "etc/unrelated-system-setting": "unrelated=true\n",
    }
    for relative, value in sentinels.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(value, encoding="utf-8")
    return sentinels


def assert_sentinels(root: Path, sentinels: dict[str, str]) -> None:
    for relative, expected in sentinels.items():
        path = root / relative
        if not path.exists() or path.read_text(encoding="utf-8") != expected:
            raise TestFailure(f"unowned state was changed or removed: /{relative}")


def assert_payload(root: Path, files: set[str], helper_contents: bytes) -> None:
    for relative in REQUIRED_INTEGRATION:
        if relative not in files or not (root / relative).is_file():
            raise TestFailure(f"installed package is missing /{relative}")
    helper = root / "usr/libexec/rog-helper-privileged"
    if helper.read_bytes() != helper_contents:
        raise TestFailure("installed privileged helper does not match the current package")
    helper_stat = helper.stat()
    if (helper_stat.st_uid, helper_stat.st_gid, helper_stat.st_mode & 0o777) != (0, 0, 0o755):
        raise TestFailure("privileged helper must be root:root mode 0755")
    for relative in REQUIRED_INTEGRATION[1:]:
        stat = (root / relative).stat()
        if (stat.st_uid, stat.st_gid, stat.st_mode & 0o777) != (0, 0, 0o644):
            raise TestFailure(f"integration file must be root:root mode 0644: /{relative}")


def assert_package_state(root: Path, expected_version: str | None) -> None:
    admin = root / "var/lib/dpkg"
    result = subprocess.run(
        ["dpkg-query", f"--admindir={admin}", "--show", "--showformat=${Status}\n${Version}", "rog-helper"],
        text=True,
        capture_output=True,
    )
    if expected_version is None:
        if result.returncode == 0:
            raise TestFailure("rog-helper remains registered after purge")
        return
    if result.returncode or result.stdout.splitlines() != ["install ok installed", expected_version]:
        raise TestFailure(f"unexpected dpkg package state: {result.stdout} {result.stderr}")


def make_previous_deb(work: Path) -> Path:
    tree = work / "previous-package"
    (tree / "DEBIAN").mkdir(parents=True)
    (tree / "usr/libexec").mkdir(parents=True)
    (tree / "DEBIAN/control").write_text(
        "Package: rog-helper\n"
        "Version: 0.0.0-1\n"
        "Architecture: amd64\n"
        "Maintainer: ROG Helper CI <ci@example.invalid>\n"
        "Description: synthetic prior helper for lifecycle testing\n",
        encoding="utf-8",
    )
    helper = tree / "usr/libexec/rog-helper-privileged"
    helper.write_text("#!/bin/sh\n# synthetic previous helper\n", encoding="utf-8")
    helper.chmod(0o755)
    output = work / "rog-helper-previous.deb"
    run("dpkg-deb", "--build", str(tree), str(output))
    return output


def dpkg(root: Path, *args: str) -> str:
    env = os.environ.copy()
    for name in ("DPKG_FORCE", "DPKG_ROOT", "DPKG_ADMINDIR"):
        env.pop(name, None)
    result = subprocess.run(
        ["dpkg", f"--root={root}", "--force-depends", *args],
        text=True,
        capture_output=True,
        env=env,
    )
    if result.returncode:
        raise TestFailure(
            f"dpkg transaction failed ({result.returncode}): {' '.join(args)}\n"
            f"stdout:\n{result.stdout}\nstderr:\n{result.stderr}"
        )
    return result.stdout


def exercise(
    deb: Path, payload_tree: Path, payload: set[str], conffiles: set[str], work: Path
) -> None:
    version = run("dpkg-deb", "--field", str(deb), "Version").strip()
    helper_contents = (payload_tree / "usr/libexec/rog-helper-privileged").read_bytes()

    fresh_root = work / "fresh-root"
    fresh_root.mkdir()
    initialize_root(fresh_root)
    fresh_sentinels = seed_unowned_state(fresh_root)
    dpkg(fresh_root, "--install", str(deb))
    assert_package_state(fresh_root, version)
    assert_payload(fresh_root, payload, helper_contents)
    assert_sentinels(fresh_root, fresh_sentinels)
    fresh_log_path = fresh_root / "var/tmp/rog-helper-dpkg-lifecycle.log"
    fresh_log = fresh_log_path.read_text()
    if "systemctl <try-restart>" in fresh_log:
        raise TestFailure("fresh install unexpectedly restarted the helper")
    print("dpkg fresh install/configure PASS")

    upgrade_root = work / "upgrade-root"
    upgrade_root.mkdir()
    initialize_root(upgrade_root)
    upgrade_sentinels = seed_unowned_state(upgrade_root)
    previous = make_previous_deb(work)
    dpkg(upgrade_root, "--install", str(previous))
    (upgrade_root / "var/tmp/rog-helper-service-active").touch()
    log = upgrade_root / "var/tmp/rog-helper-dpkg-lifecycle.log"
    log.write_text("", encoding="utf-8")
    dpkg(upgrade_root, "--install", str(deb))
    assert_package_state(upgrade_root, version)
    assert_payload(upgrade_root, payload, helper_contents)
    assert_sentinels(upgrade_root, upgrade_sentinels)
    upgrade_log = log.read_text(encoding="utf-8")
    if "systemctl <try-restart> <rog-helper-privileged.service>" not in upgrade_log:
        raise TestFailure("upgrade did not restart the already-active helper")
    print("dpkg upgrade PASS (synthetic 0.0.0-1 -> current)")

    dpkg(fresh_root, "--remove", "rog-helper")
    for relative in REQUIRED_INTEGRATION:
        if (fresh_root / relative).exists():
            raise TestFailure(f"package integration remains after remove: /{relative}")
    for relative in payload - conffiles:
        if (fresh_root / relative).is_file():
            raise TestFailure(f"package file remains after remove: /{relative}")
    for relative in conffiles:
        if not (fresh_root / relative).is_file():
            raise TestFailure(f"conffile was removed before purge: /{relative}")
    assert_sentinels(fresh_root, fresh_sentinels)
    if "systemctl <stop> <rog-helper-privileged.service>" not in fresh_log_path.read_text(encoding="utf-8"):
        raise TestFailure("remove did not request stopping the privileged helper")
    print("dpkg remove PASS")

    dpkg(fresh_root, "--purge", "rog-helper")
    for relative in conffiles:
        if (fresh_root / relative).exists():
            raise TestFailure(f"conffile remains after purge: /{relative}")
    for relative in payload:
        path = fresh_root / relative
        if path.is_file() or path.is_symlink():
            raise TestFailure(f"package file remains after purge: /{relative}")
    assert_package_state(fresh_root, None)
    assert_sentinels(fresh_root, fresh_sentinels)
    print("dpkg purge PASS")


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("deb", nargs="?", type=Path, default=current_deb_path())
    args = parser.parse_args()
    if os.geteuid() != 0:
        print("run this isolated dpkg lifecycle test as root", file=sys.stderr)
        return 2
    if not args.deb.is_file():
        print(f"package not found: {args.deb}", file=sys.stderr)
        return 2
    for command in ("dpkg", "dpkg-deb", "dpkg-query", "ldd"):
        if shutil.which(command) is None:
            print(f"{command} is required", file=sys.stderr)
            return 2
    try:
        with tempfile.TemporaryDirectory(prefix="rog-helper-dpkg-lifecycle-") as temporary:
            work = Path(temporary)
            payload_tree, payload, conffiles = package_files(args.deb.resolve(), work / "inspect")
            exercise(args.deb.resolve(), payload_tree, payload, conffiles, work)
    except (OSError, subprocess.CalledProcessError, TestFailure) as error:
        print(f"DEB dpkg lifecycle test failed: {error}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
