#!/usr/bin/env python3
"""Exercise a ROG Helper Debian package lifecycle in an isolated filesystem.

This is deliberately a package-ownership model, not an apt replacement.  It
extracts packages with dpkg-deb, models dpkg's file replacement/removal rules,
and never executes package binaries or maintainer scripts.  The separate
maintainer-script test covers those scripts with mocked system commands.
"""

from __future__ import annotations

import argparse
import hashlib
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile


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


class Package:
    def __init__(self, deb: Path, work: Path, label: str) -> None:
        self.deb = deb.resolve()
        self.tree = work / f"{label}-tree"
        self.control = work / f"{label}-control"
        self.tree.mkdir()
        self.control.mkdir()
        run("dpkg-deb", "--extract", str(self.deb), str(self.tree))
        run("dpkg-deb", "--control", str(self.deb), str(self.control))
        self.version = query_field(self.deb, "Version")
        self.package = query_field(self.deb, "Package")
        self.owned = file_manifest(self.tree)
        self.conffiles = read_conffiles(self.control / "conffiles")


def run(*args: str) -> str:
    completed = subprocess.run(
        args,
        check=True,
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
        text=True,
    )
    return completed.stdout


def query_field(deb: Path, field: str) -> str:
    return run("dpkg-deb", "--field", str(deb), field).strip()


def file_manifest(tree: Path) -> set[str]:
    return {
        path.relative_to(tree).as_posix()
        for path in tree.rglob("*")
        if path.is_file() or path.is_symlink()
    }


def read_conffiles(path: Path) -> set[str]:
    if not path.exists():
        return set()
    return {
        line.strip().split()[0].lstrip("/")
        for line in path.read_text(encoding="utf-8").splitlines()
        if line.strip() and not line.lstrip().startswith("#")
    }


def sha256(path: Path) -> str:
    digest = hashlib.sha256()
    with path.open("rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            digest.update(chunk)
    return digest.hexdigest()


def install_tree(source: Path, root: Path) -> None:
    for source_path in sorted(source.rglob("*")):
        relative = source_path.relative_to(source)
        destination = root / relative
        if source_path.is_dir() and not source_path.is_symlink():
            destination.mkdir(parents=True, exist_ok=True)
            continue
        destination.parent.mkdir(parents=True, exist_ok=True)
        if destination.is_symlink() or destination.is_file():
            destination.unlink()
        elif destination.exists():
            raise TestFailure(f"cannot replace directory with package file: /{relative}")
        if source_path.is_symlink():
            destination.symlink_to(os.readlink(source_path))
        else:
            shutil.copy2(source_path, destination, follow_symlinks=False)


def remove_owned(root: Path, owned: set[str], preserve: set[str]) -> None:
    for relative in sorted(owned - preserve, reverse=True):
        target = root / relative
        if target.is_symlink() or target.is_file():
            target.unlink()

    # dpkg removes empty package directories, but must not recursively remove
    # directories containing administrator- or user-created files.
    parents = {
        root / parent
        for item in owned
        for parent in Path(item).parents
        if parent != Path(".")
    }
    for directory in sorted(parents, key=lambda path: len(path.parts), reverse=True):
        try:
            directory.rmdir()
        except (FileNotFoundError, OSError):
            pass


def assert_exists(root: Path, relative: str) -> None:
    if not (root / relative).exists():
        raise TestFailure(f"expected package path is absent: /{relative}")


def assert_absent(root: Path, relative: str) -> None:
    path = root / relative
    if path.exists() or path.is_symlink():
        raise TestFailure(f"removed package path remains: /{relative}")


def seed_unowned_state(root: Path) -> dict[str, str]:
    sentinels = {
        "home/test/.config/rog-helper/settings.toml": "user_setting=true\n",
        "usr/share/doc/rog-helper/local-administrator-note.txt": "keep me\n",
        "etc/unrelated-system-setting": "unrelated=true\n",
    }
    for relative, contents in sentinels.items():
        path = root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(contents, encoding="utf-8")
    return sentinels


def assert_sentinels(root: Path, sentinels: dict[str, str]) -> None:
    for relative, expected in sentinels.items():
        path = root / relative
        if not path.exists() or path.read_text(encoding="utf-8") != expected:
            raise TestFailure(f"unowned state was changed or removed: /{relative}")


def make_synthetic_v1(current: Package, work: Path) -> tuple[Path, set[str]]:
    tree = work / "synthetic-api-v1-tree"
    shutil.copytree(current.tree, tree, symlinks=True)
    helper = tree / "usr/libexec/rog-helper-privileged"
    helper.write_text("#!/bin/sh\n# synthetic stale privileged API v1 helper\n", encoding="utf-8")
    helper.chmod(0o755)
    return tree, file_manifest(tree)


def validate_current_package(package: Package) -> None:
    if package.package != "rog-helper":
        raise TestFailure(f"unexpected package name: {package.package}")
    for relative in REQUIRED_INTEGRATION:
        if relative not in package.owned:
            raise TestFailure(f"current package does not own /{relative}")
    helper = package.tree / "usr/libexec/rog-helper-privileged"
    if b"SetAuraEffect" not in helper.read_bytes():
        raise TestFailure("current privileged helper does not expose API-v2 SetAuraEffect")


def exercise(current: Package, previous: Package | None, root: Path, work: Path) -> None:
    sentinels = seed_unowned_state(root)
    current_helper_hash = sha256(current.tree / "usr/libexec/rog-helper-privileged")

    # Clean install.
    install_tree(current.tree, root)
    for relative in REQUIRED_INTEGRATION:
        assert_exists(root, relative)
    assert_sentinels(root, sentinels)
    print("fresh install PASS")

    # Reinstall must repair a damaged/stale package-owned helper.
    installed_helper = root / "usr/libexec/rog-helper-privileged"
    installed_helper.write_text("stale helper\n", encoding="utf-8")
    install_tree(current.tree, root)
    if sha256(installed_helper) != current_helper_hash:
        raise TestFailure("reinstall did not restore the current privileged helper")
    assert_sentinels(root, sentinels)
    print("reinstall PASS")

    # Start an independent upgrade root with either a real previous package or
    # a deterministic API-v1 helper fixture based on the current payload.
    upgrade_root = work / "upgrade-root"
    upgrade_root.mkdir()
    upgrade_sentinels = seed_unowned_state(upgrade_root)
    if previous is None:
        previous_tree, previous_owned = make_synthetic_v1(current, work)
        previous_version = "synthetic-api-v1"
    else:
        previous_tree, previous_owned = previous.tree, previous.owned
        previous_version = previous.version
    install_tree(previous_tree, upgrade_root)
    before_hash = sha256(upgrade_root / "usr/libexec/rog-helper-privileged")
    if before_hash == current_helper_hash:
        raise TestFailure("upgrade fixture helper is identical to the current helper")
    remove_owned(upgrade_root, previous_owned - current.owned, set())
    install_tree(current.tree, upgrade_root)
    if sha256(upgrade_root / "usr/libexec/rog-helper-privileged") != current_helper_hash:
        raise TestFailure("upgrade left the previous privileged helper installed")
    assert_sentinels(upgrade_root, upgrade_sentinels)
    print(f"upgrade PASS ({previous_version} -> {current.version})")

    # Normal remove deletes package-owned integration but preserves conffiles
    # and all unowned state, including files nested under package directories.
    remove_owned(root, current.owned, current.conffiles)
    for relative in REQUIRED_INTEGRATION:
        assert_absent(root, relative)
    assert_sentinels(root, sentinels)
    print("remove PASS")

    # Purge removes package conffiles only; it is still forbidden to touch
    # arbitrary home or administrator-created files.
    remove_owned(root, current.conffiles, set())
    for relative in current.conffiles:
        assert_absent(root, relative)
    assert_sentinels(root, sentinels)
    print("purge PASS")


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "deb",
        nargs="?",
        type=Path,
        default=Path("dist/rog-helper_0.3.0_amd64.deb"),
        help="current package to test (default: dist/rog-helper_0.3.0_amd64.deb)",
    )
    parser.add_argument(
        "--previous",
        type=Path,
        help="real previous package; otherwise a deterministic API-v1 fixture is used",
    )
    return parser.parse_args()


def main() -> int:
    args = parse_args()
    if not args.deb.is_file():
        print(f"package not found: {args.deb}", file=sys.stderr)
        return 2
    if args.previous is not None and not args.previous.is_file():
        print(f"previous package not found: {args.previous}", file=sys.stderr)
        return 2
    if shutil.which("dpkg-deb") is None:
        print("dpkg-deb is required", file=sys.stderr)
        return 2

    try:
        with tempfile.TemporaryDirectory(prefix="rog-helper-deb-lifecycle-") as temporary:
            work = Path(temporary)
            current = Package(args.deb, work, "current")
            previous = Package(args.previous, work, "previous") if args.previous else None
            validate_current_package(current)
            root = work / "clean-root"
            root.mkdir()
            exercise(current, previous, root, work)
        print("isolated package lifecycle PASS")
        print("note: maintainer scripts, apt dependency resolution, and hardware require separate tests")
        return 0
    except (OSError, subprocess.CalledProcessError, TestFailure) as error:
        print(f"isolated package lifecycle FAIL: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
