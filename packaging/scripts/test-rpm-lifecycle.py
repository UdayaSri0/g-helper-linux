#!/usr/bin/env python3
"""Exercise RPM install, upgrade, and erase in disposable install roots."""

from pathlib import Path
import re
import shutil
import subprocess
import sys
import tempfile


INTEGRATION_FILES = {
    "usr/libexec/rog-helper-privileged": 0o755,
    "usr/lib/systemd/system/rog-helper-privileged.service": 0o644,
    "usr/lib/systemd/user/rog-helperd.service": 0o644,
    "usr/lib/udev/rules.d/60-rog-helper-aura.rules": 0o644,
    "usr/share/dbus-1/services/io.github.roghelper.Daemon.service": 0o644,
    "usr/share/dbus-1/system-services/io.github.roghelper.Privileged.service": 0o644,
    "usr/share/dbus-1/system.d/io.github.roghelper.Privileged.conf": 0o644,
    "usr/share/polkit-1/actions/io.github.roghelper.policy": 0o644,
}
LOG_PATH = "/var/tmp/rog-helper-rpm-scriptlets.log"


def run(*args: str) -> str:
    result = subprocess.run(
        args, check=True, stdout=subprocess.PIPE, stderr=subprocess.PIPE, text=True
    )
    return result.stdout


def copy_shell_runtime(root: Path) -> None:
    """Prepare only /bin/sh and its shared libraries for RPM's chrooted scriptlets."""
    shell = Path("/bin/sh").resolve()
    destinations = {shell}
    for line in run("ldd", str(shell)).splitlines():
        match = re.search(r"=>\s+(/[^\s]+)|^\s*(/[^\s]+)", line)
        if match:
            destinations.add(Path(match.group(1) or match.group(2)))

    for source in destinations:
        target = root / source.relative_to("/")
        target.parent.mkdir(parents=True, exist_ok=True)
        shutil.copy2(source, target)

    (root / "bin").mkdir(parents=True, exist_ok=True)
    shutil.copy2(shell, root / "bin/sh")

    (root / "dev").mkdir(parents=True, exist_ok=True)
    (root / "dev/null").touch()
    (root / "var/tmp").mkdir(parents=True, exist_ok=True)
    (root / "bin/systemctl").write_text(
        f'#!/bin/sh\nprintf "%s\\n" "systemctl $*" >> {LOG_PATH}\n', encoding="utf-8"
    )
    (root / "bin/udevadm").write_text(
        f'#!/bin/sh\nprintf "%s\\n" "udevadm $*" >> {LOG_PATH}\n', encoding="utf-8"
    )
    (root / "bin/systemctl").chmod(0o755)
    (root / "bin/udevadm").chmod(0o755)


def initialize_root(root: Path) -> None:
    root.mkdir()
    copy_shell_runtime(root)
    run("rpm", "--root", str(root), "--initdb")


def make_previous_rpm(work: Path) -> Path:
    top = work / "previous-rpmbuild"
    for directory in ("BUILD", "BUILDROOT", "RPMS", "SOURCES", "SPECS", "SRPMS"):
        (top / directory).mkdir(parents=True, exist_ok=True)
    spec = top / "SPECS/rog-helper-previous.spec"
    spec.write_text(
        """Name: rog-helper
Version: 0.0.0
Release: 1
Summary: Synthetic previous ROG Helper package for lifecycle testing
License: MIT
BuildArch: x86_64

%description
Synthetic previous package used only by the RPM lifecycle test.

%install
mkdir -p %{buildroot}/usr/share/rog-helper
printf 'previous package\\n' > %{buildroot}/usr/share/rog-helper/previous-version

%files
/usr/share/rog-helper/previous-version
""",
        encoding="utf-8",
    )
    run("rpmbuild", "--define", f"_topdir {top}", "-bb", str(spec))
    rpms = list((top / "RPMS/x86_64").glob("rog-helper-0.0.0-1.x86_64.rpm"))
    if len(rpms) != 1:
        raise RuntimeError(f"expected one synthetic previous RPM, found {len(rpms)}")
    return rpms[0]


def assert_installed_payload(root: Path, rpm: Path) -> str:
    version = run("rpm", "-qp", "--qf", "%{VERSION}", str(rpm)).strip()
    if run("rpm", "--root", str(root), "-q", "--qf", "%{VERSION}", "rog-helper").strip() != version:
        raise RuntimeError("installed RPM version does not match the artifact")
    for relative, expected_mode in INTEGRATION_FILES.items():
        path = root / relative
        if not path.is_file():
            raise RuntimeError(f"installed package path is missing: /{relative}")
        metadata = path.stat()
        mode = metadata.st_mode & 0o7777
        if metadata.st_uid != 0 or metadata.st_gid != 0 or mode != expected_mode:
            raise RuntimeError(
                f"installed package path has wrong owner/mode: /{relative} "
                f"({metadata.st_uid}:{metadata.st_gid} {mode:04o})"
            )

    if "rog-helper" not in run("rpm", "--root", str(root), "-qf", "/usr/libexec/rog-helper-privileged"):
        raise RuntimeError("RPM database does not own the installed privileged helper")
    return version


def read_scriptlet_log(root: Path) -> list[str]:
    log = root / LOG_PATH.lstrip("/")
    return log.read_text(encoding="utf-8").splitlines() if log.exists() else []


def main() -> int:
    if len(sys.argv) != 2:
        raise SystemExit("usage: test-rpm-lifecycle.py PACKAGE.rpm")
    rpm = Path(sys.argv[1]).resolve()
    if not rpm.is_file():
        raise SystemExit(f"RPM artifact not found: {rpm}")

    with tempfile.TemporaryDirectory(prefix="rog-helper-rpm-lifecycle-") as temporary:
        work = Path(temporary)
        fresh_root = work / "fresh-root"
        initialize_root(fresh_root)
        sentinel = fresh_root / "home/test/.config/rog-helper/settings.toml"
        sentinel.parent.mkdir(parents=True, exist_ok=True)
        sentinel.write_text("preserve=true\n", encoding="utf-8")
        run("rpm", "--root", str(fresh_root), "--nodeps", "-Uvh", str(rpm))
        version = assert_installed_payload(fresh_root, rpm)
        if sentinel.read_text(encoding="utf-8") != "preserve=true\n":
            raise RuntimeError("fresh RPM install changed unowned user configuration")
        fresh_calls = read_scriptlet_log(fresh_root)
        expected_install = {
            "systemctl daemon-reload",
            "udevadm control --reload-rules",
            "udevadm trigger --subsystem-match=hidraw --action=change",
        }
        if not expected_install.issubset(fresh_calls) or any(
            "try-restart" in call or "systemctl stop" in call for call in fresh_calls
        ):
            raise RuntimeError(f"unexpected fresh-install scriptlet calls: {fresh_calls}")
        print(f"fresh RPM install PASS ({version}); service and udev calls were mocked")

        upgrade_root = work / "upgrade-root"
        initialize_root(upgrade_root)
        upgrade_sentinel = upgrade_root / "home/test/.config/rog-helper/settings.toml"
        upgrade_sentinel.parent.mkdir(parents=True, exist_ok=True)
        upgrade_sentinel.write_text("preserve-upgrade=true\n", encoding="utf-8")
        previous = make_previous_rpm(work)
        run("rpm", "--root", str(upgrade_root), "--nodeps", "-Uvh", str(previous))
        (upgrade_root / LOG_PATH.lstrip("/")).unlink(missing_ok=True)
        run("rpm", "--root", str(upgrade_root), "--nodeps", "-Uvh", str(rpm))
        assert_installed_payload(upgrade_root, rpm)
        if upgrade_sentinel.read_text(encoding="utf-8") != "preserve-upgrade=true\n":
            raise RuntimeError("RPM upgrade changed unowned user configuration")
        upgrade_calls = read_scriptlet_log(upgrade_root)
        if not expected_install.issubset(upgrade_calls) or (
            "systemctl try-restart rog-helper-privileged.service" not in upgrade_calls
        ):
            raise RuntimeError(f"RPM upgrade did not refresh an already-installed helper: {upgrade_calls}")
        if (upgrade_root / "usr/share/rog-helper/previous-version").exists():
            raise RuntimeError("RPM upgrade left the previous package payload installed")
        print("RPM upgrade PASS; service and udev calls were mocked")

        (fresh_root / LOG_PATH.lstrip("/")).unlink(missing_ok=True)
        run("rpm", "--root", str(fresh_root), "--nodeps", "-e", "rog-helper")
        erase_calls = read_scriptlet_log(fresh_root)
        if (
            "systemctl stop rog-helper-privileged.service" not in erase_calls
            or not expected_install.issubset(erase_calls)
        ):
            raise RuntimeError(f"RPM erase did not stop the helper service: {erase_calls}")
        if (fresh_root / "usr/libexec/rog-helper-privileged").exists():
            raise RuntimeError("RPM erase left the privileged helper installed")
        if any((fresh_root / relative).exists() for relative in INTEGRATION_FILES):
            raise RuntimeError("RPM erase left package-owned integration files installed")
        if sentinel.read_text(encoding="utf-8") != "preserve=true\n":
            raise RuntimeError("RPM erase removed unowned user configuration")
        if run("rpm", "--root", str(fresh_root), "-qa").strip():
            raise RuntimeError("RPM erase left an installed package in the disposable root")
        print("RPM erase PASS; service and udev calls were mocked")

    return 0


if __name__ == "__main__":
    raise SystemExit(main())
