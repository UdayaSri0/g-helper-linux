#!/usr/bin/env python3
"""Validate the files and rendered metadata in an installed tree or Debian package."""

from __future__ import annotations

import re
import subprocess
import sys
import tempfile
from contextlib import nullcontext
from pathlib import Path


REQUIRED_FILES = {
    "usr/bin/rog-helper-ui": 0o755,
    "usr/bin/rog-helperd": 0o755,
    "usr/bin/rog-helper": 0o755,
    "usr/libexec/rog-helper-privileged": 0o755,
    "usr/share/applications/rog-helper.desktop": 0o644,
    "usr/share/metainfo/io.github.roghelper.UI.metainfo.xml": 0o644,
    "usr/share/dbus-1/services/io.github.roghelper.Daemon.service": 0o644,
    "usr/lib/systemd/user/rog-helperd.service": 0o644,
    "usr/share/dbus-1/system-services/io.github.roghelper.Privileged.service": 0o644,
    "usr/share/dbus-1/system.d/io.github.roghelper.Privileged.conf": 0o644,
    "usr/lib/systemd/system/rog-helper-privileged.service": 0o644,
    "usr/share/polkit-1/actions/io.github.roghelper.policy": 0o644,
    "usr/lib/udev/rules.d/60-rog-helper-aura.rules": 0o644,
    "usr/share/icons/hicolor/scalable/actions/rog-fan-symbolic.svg": 0o644,
    "usr/share/doc/rog-helper/README.Debian": 0o644,
    "usr/share/doc/rog-helper/LICENSE-APACHE": 0o644,
    "usr/share/doc/rog-helper/LICENSE-MIT": 0o644,
    "usr/share/doc/rog-helper/copyright": 0o644,
}
REQUIRED_ICON_SIZES = (16, 24, 32, 48, 64, 128, 256, 512)
TOKEN = re.compile(r"@[A-Z][A-Z0-9_]*@")
METADATA_SUFFIXES = {".conf", ".desktop", ".policy", ".rules", ".service", ".xml"}


def fail(message: str) -> None:
    raise AssertionError(message)


def validate(root: Path, privileged_only: bool) -> None:
    if privileged_only:
        expected = {
            path: mode
            for path, mode in REQUIRED_FILES.items()
            if "Privileged" in path
            or "rog-helper-privileged" in path
            or "polkit-1" in path
            or "udev/rules.d" in path
        }
    else:
        expected = dict(REQUIRED_FILES)
        for size in REQUIRED_ICON_SIZES:
            expected[f"usr/share/icons/hicolor/{size}x{size}/apps/rog-helper.png"] = 0o644

    for relative, wanted_mode in expected.items():
        target = root / relative
        if not target.is_file():
            fail(f"missing packaged file: /{relative}")
        actual_mode = target.stat().st_mode & 0o777
        if actual_mode != wanted_mode:
            fail(
                f"incorrect mode for /{relative}: {actual_mode:04o} "
                f"(expected {wanted_mode:04o})"
            )

    metadata_roots = (
        root / "usr/lib/systemd",
        root / "usr/lib/udev/rules.d",
        root / "usr/share/dbus-1",
        root / "usr/share/polkit-1/actions",
        root / "usr/share/applications",
        root / "usr/share/metainfo",
    )
    for metadata_root in metadata_roots:
        if not metadata_root.exists():
            continue
        for target in metadata_root.rglob("*"):
            if not target.is_file() or target.suffix not in METADATA_SUFFIXES:
                continue
            contents = target.read_text(encoding="utf-8")
            unresolved = sorted(set(TOKEN.findall(contents)))
            if unresolved:
                relative = target.relative_to(root)
                fail(
                    f"unresolved template token(s) in /{relative}: "
                    + ", ".join(unresolved)
                )

    privileged_dbus = (
        root / "usr/share/dbus-1/system-services/io.github.roghelper.Privileged.service"
    ).read_text(encoding="utf-8")
    if "Exec=/usr/libexec/rog-helper-privileged" not in privileged_dbus:
        fail("privileged D-Bus activation does not use the packaged helper")

    privileged_systemd = (
        root / "usr/lib/systemd/system/rog-helper-privileged.service"
    ).read_text(encoding="utf-8")
    if "ExecStart=/usr/libexec/rog-helper-privileged" not in privileged_systemd:
        fail("privileged systemd unit does not use the packaged helper")

    if not privileged_only:
        daemon_dbus = (
            root / "usr/share/dbus-1/services/io.github.roghelper.Daemon.service"
        ).read_text(encoding="utf-8")
        if "Exec=/usr/bin/rog-helperd" not in daemon_dbus:
            fail("session D-Bus activation does not use the packaged daemon")
        daemon_systemd = (
            root / "usr/lib/systemd/user/rog-helperd.service"
        ).read_text(encoding="utf-8")
        if "ExecStart=/usr/bin/rog-helperd" not in daemon_systemd:
            fail("user systemd unit does not use the packaged daemon")

    aura_rule = (root / "usr/lib/udev/rules.d/60-rog-helper-aura.rules").read_text(
        encoding="utf-8"
    )
    for forbidden in ('TAG+="uaccess"', "MODE=", "GROUP="):
        if forbidden in aura_rule:
            fail(f"Aura udev rule broadens device access: {forbidden}")


def main() -> int:
    args = sys.argv[1:]
    privileged_only = False
    if args and args[0] == "--privileged-only":
        privileged_only = True
        args.pop(0)
    if len(args) != 1:
        print(
            f"usage: {Path(sys.argv[0]).name} [--privileged-only] "
            "STAGED_ROOT|PACKAGE.deb",
            file=sys.stderr,
        )
        return 2

    source = Path(args[0]).resolve()
    if source.is_dir():
        context = nullcontext(source)
    elif source.is_file() and source.suffix == ".deb":
        temporary = tempfile.TemporaryDirectory(prefix="rog-helper-deb-check-")

        class ExtractedPackage:
            def __enter__(self) -> Path:
                root = Path(temporary.__enter__())
                subprocess.run(
                    ["dpkg-deb", "--extract", str(source), str(root)], check=True
                )
                return root

            def __exit__(self, *args: object) -> None:
                temporary.__exit__(*args)

        context = ExtractedPackage()
    else:
        print(f"not a staged tree or Debian package: {source}", file=sys.stderr)
        return 2

    try:
        with context as root:
            validate(root, privileged_only)
    except (AssertionError, OSError, subprocess.CalledProcessError) as error:
        print(f"package payload validation failed: {error}", file=sys.stderr)
        return 1

    print(f"package payload OK: {source}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
