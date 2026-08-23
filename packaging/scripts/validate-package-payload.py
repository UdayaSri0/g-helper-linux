#!/usr/bin/env python3
"""Validate the files and rendered metadata in an installed tree or Debian package."""

from __future__ import annotations

import re
import shutil
import subprocess
import sys
import tarfile
import tempfile
import xml.etree.ElementTree as ET
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
REQUIRED_DEPENDS = {"dbus", "dbus-user-session", "policykit-1", "systemd", "udev"}
REQUIRED_RECOMMENDS = {"desktop-file-utils", "gtk-update-icon-cache"}
REQUIRED_SUGGESTS = {"asusd", "pciutils", "supergfxd", "upower"}


def fail(message: str) -> None:
    raise AssertionError(message)


def parse_service_file(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    for raw_line in path.read_text(encoding="utf-8").splitlines():
        line = raw_line.strip()
        if line and not line.startswith(("#", ";", "[")) and "=" in line:
            key, value = line.split("=", 1)
            values[key] = value
    return values


def parse_control_file(path: Path) -> dict[str, str]:
    values: dict[str, str] = {}
    current = ""
    for line in path.read_text(encoding="utf-8").splitlines():
        if line[:1].isspace() and current:
            values[current] += "\n" + line.strip()
        elif ":" in line:
            current, value = line.split(":", 1)
            values[current] = value.strip()
    return values


def require_not_writable_by_non_root(path: Path, root: Path) -> None:
    mode = path.stat().st_mode & 0o7777
    if mode & 0o022:
        fail(f"security-sensitive path is group/world writable: /{path.relative_to(root)}")


def validate_archive_ownership(package: Path, command: str) -> None:
    process = subprocess.Popen(
        ["dpkg-deb", command, str(package)],
        stdout=subprocess.PIPE,
        stderr=subprocess.PIPE,
    )
    assert process.stdout is not None
    try:
        with tarfile.open(fileobj=process.stdout, mode="r|*") as archive:
            for member in archive:
                relative = member.name.removeprefix("./")
                if member.uid != 0 or member.gid != 0:
                    fail(
                        f"package member is not root:root: /{relative} "
                        f"({member.uid}:{member.gid})"
                    )
                if member.mode & 0o6000:
                    fail(f"setuid/setgid package member is forbidden: /{relative}")
                if relative.startswith("etc/sudoers"):
                    fail(f"sudoers package member is forbidden: /{relative}")
                if relative == "usr/libexec/rog-helper-privileged" and (
                    member.mode & 0o7777
                ) != 0o755:
                    fail("privileged helper archive mode must be exactly 0755")
        stderr = (
            process.stderr.read().decode("utf-8", errors="replace")
            if process.stderr
            else ""
        )
        if process.wait() != 0:
            fail(f"could not inspect Debian archive ownership: {stderr.strip()}")
    except BaseException:
        process.kill()
        process.wait()
        raise


def relationship_names(value: str) -> set[str]:
    return {
        match.group(1).lower()
        for clause in value.split(",")
        for alternative in clause.split("|")
        if (match := re.match(r"\s*([a-z0-9][a-z0-9+.-]*)", alternative, re.I))
    }


def validate_control(root: Path) -> None:
    control_dir = root / "DEBIAN"
    control_path = control_dir / "control"
    if not control_path.is_file():
        fail("missing Debian control metadata")
    control = parse_control_file(control_path)
    if control.get("Package") != "rog-helper":
        fail("Debian package name must be rog-helper")
    for field in ("Version", "Architecture", "Maintainer", "Description"):
        if not control.get(field):
            fail(f"Debian control field is missing: {field}")
    for field, required in (
        ("Depends", REQUIRED_DEPENDS),
        ("Recommends", REQUIRED_RECOMMENDS),
        ("Suggests", REQUIRED_SUGGESTS),
    ):
        missing = required - relationship_names(control.get(field, ""))
        if missing:
            fail(f"Debian {field} is missing: {', '.join(sorted(missing))}")

    for script_name in ("postinst", "postrm"):
        script = control_dir / script_name
        if not script.is_file():
            fail(f"missing Debian maintainer script: {script_name}")
        if script.stat().st_mode & 0o7777 != 0o755:
            fail(f"Debian maintainer script mode must be 0755: {script_name}")
        contents = script.read_text(encoding="utf-8")
        for forbidden in ("systemctl --user", "sudo ", "/etc/sudoers", "chmod 666"):
            if forbidden in contents:
                fail(f"unsafe Debian maintainer script content in {script_name}: {forbidden}")


def run_validator(command: list[str], description: str) -> None:
    completed = subprocess.run(command, text=True, capture_output=True)
    if completed.returncode != 0:
        details = (completed.stdout + completed.stderr).strip()
        fail(f"{description} failed: {details}")


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
        actual_mode = target.stat().st_mode & 0o7777
        if actual_mode != wanted_mode:
            fail(
                f"incorrect mode for /{relative}: {actual_mode:04o} "
                f"(expected {wanted_mode:04o})"
            )

    for relative in (
        "usr",
        "usr/libexec",
        "usr/libexec/rog-helper-privileged",
        "usr/lib/systemd/system",
        "usr/share/dbus-1/system-services",
        "usr/share/dbus-1/system.d",
        "usr/share/polkit-1/actions",
        "usr/lib/udev/rules.d",
    ):
        require_not_writable_by_non_root(root / relative, root)

    forbidden_roots = (root / "etc/sudoers.d",)
    for forbidden_root in forbidden_roots:
        if forbidden_root.exists() and any(forbidden_root.iterdir()):
            fail("package must not install sudoers entries")

    for target in root.rglob("*"):
        if target.is_file():
            try:
                contents = target.read_text(encoding="utf-8")
            except UnicodeDecodeError:
                continue
            unresolved = sorted(set(re.findall(r"@[A-Z][A-Z0-9_]*@", contents)))
            if unresolved:
                relative = target.relative_to(root)
                fail(
                    f"unresolved template token(s) in /{relative}: "
                    + ", ".join(unresolved)
                )

    if (root / "DEBIAN").exists() and not privileged_only:
        validate_control(root)

    privileged_dbus = (
        root / "usr/share/dbus-1/system-services/io.github.roghelper.Privileged.service"
    )
    privileged_dbus_values = parse_service_file(privileged_dbus)
    if privileged_dbus_values.get("Name") != "io.github.roghelper.Privileged":
        fail("privileged D-Bus activation name is incorrect")
    if privileged_dbus_values.get("Exec") != "/usr/libexec/rog-helper-privileged":
        fail("privileged D-Bus activation does not use the packaged helper")
    if privileged_dbus_values.get("User") != "root":
        fail("privileged D-Bus activation must run as root")
    if privileged_dbus_values.get("SystemdService") != "rog-helper-privileged.service":
        fail("privileged D-Bus activation does not use the packaged system unit")

    bus_policy_path = (
        root / "usr/share/dbus-1/system.d/io.github.roghelper.Privileged.conf"
    )
    bus_policy = ET.parse(bus_policy_path).getroot()
    root_policy = bus_policy.find("./policy[@user='root']")
    default_policy = bus_policy.find("./policy[@context='default']")
    if root_policy is None or root_policy.find(
        "./allow[@own='io.github.roghelper.Privileged']"
    ) is None:
        fail("only the root policy must own the privileged D-Bus name")
    if default_policy is None or default_policy.find(
        "./allow[@send_destination='io.github.roghelper.Privileged']"
    ) is None:
        fail("privileged D-Bus send policy is missing")
    if default_policy.findall("./allow[@own]"):
        fail("default D-Bus policy must not own the privileged bus name")

    privileged_systemd = (
        root / "usr/lib/systemd/system/rog-helper-privileged.service"
    ).read_text(encoding="utf-8")
    if "ExecStart=/usr/libexec/rog-helper-privileged" not in privileged_systemd:
        fail("privileged systemd unit does not use the packaged helper")
    for required in (
        "Type=dbus",
        "BusName=io.github.roghelper.Privileged",
        "User=root",
        "Group=root",
        "NoNewPrivileges=yes",
        "PrivateTmp=yes",
        "PrivateDevices=yes",
        "BindPaths=-/dev/rog-helper-aura",
        "DeviceAllow=/dev/rog-helper-aura rw",
        "ProtectHome=yes",
        "ProtectSystem=strict",
        "ProtectKernelModules=yes",
        "RestrictAddressFamilies=AF_UNIX",
        "RestrictNamespaces=yes",
        "RestrictSUIDSGID=yes",
        "MemoryDenyWriteExecute=yes",
        "SystemCallFilter=@system-service",
    ):
        if required not in privileged_systemd:
            fail(f"privileged systemd hardening/activation drift: {required}")
    for forbidden in ("PrivateDevices=no", "hidraw*", "DeviceAllow=char-usb_device"):
        if forbidden in privileged_systemd:
            fail(f"privileged systemd unit broadens device access: {forbidden}")

    if not privileged_only:
        daemon_dbus_path = (
            root / "usr/share/dbus-1/services/io.github.roghelper.Daemon.service"
        )
        daemon_dbus = parse_service_file(daemon_dbus_path)
        if daemon_dbus.get("Name") != "io.github.roghelper.Daemon":
            fail("session D-Bus activation name is incorrect")
        if daemon_dbus.get("Exec") != "/usr/bin/rog-helperd":
            fail("session D-Bus activation does not use the packaged daemon")
        if daemon_dbus.get("SystemdService") != "rog-helperd.service":
            fail("session D-Bus activation does not use the packaged user unit")
        daemon_systemd = (
            root / "usr/lib/systemd/user/rog-helperd.service"
        ).read_text(encoding="utf-8")
        if "ExecStart=/usr/bin/rog-helperd" not in daemon_systemd:
            fail("user systemd unit does not use the packaged daemon")
        if "Type=dbus" not in daemon_systemd:
            fail("user systemd unit must use D-Bus readiness")
        if "BusName=io.github.roghelper.Daemon" not in daemon_systemd:
            fail("user systemd unit bus name is incorrect")

        desktop_path = root / "usr/share/applications/rog-helper.desktop"
        desktop = parse_service_file(desktop_path)
        if desktop.get("Type") != "Application":
            fail("desktop entry must describe an Application")
        if desktop.get("Exec") != "rog-helper-ui":
            fail("desktop entry must launch the unprivileged UI")
        if any(word in desktop.get("Exec", "").split() for word in ("sudo", "pkexec")):
            fail("desktop entry must not elevate the UI")

        metainfo_path = root / "usr/share/metainfo/io.github.roghelper.UI.metainfo.xml"
        metainfo = ET.parse(metainfo_path).getroot()
        if metainfo.get("type") != "desktop-application":
            fail("AppStream component type is incorrect")
        if metainfo.findtext("id") != "io.github.roghelper.UI":
            fail("AppStream component ID is incorrect")
        if metainfo.findtext("./launchable[@type='desktop-id']") != "rog-helper.desktop":
            fail("AppStream desktop launchable is incorrect")

        external_checks = (
            (
                "desktop-file-validate",
                ["desktop-file-validate", str(desktop_path)],
                "desktop entry validation",
            ),
            (
                "appstreamcli",
                ["appstreamcli", "validate", "--no-net", str(metainfo_path)],
                "AppStream validation",
            ),
            (
                "systemd-analyze",
                [
                    "systemd-analyze",
                    "verify",
                    str(
                        root
                        / "usr/lib/systemd/system/rog-helper-privileged.service"
                    ),
                    str(root / "usr/lib/systemd/user/rog-helperd.service"),
                ],
                "systemd unit validation",
            ),
            (
                "udevadm",
                [
                    "udevadm",
                    "verify",
                    str(root / "usr/lib/udev/rules.d/60-rog-helper-aura.rules"),
                ],
                "udev rule validation",
            ),
        )
        for executable, command, description in external_checks:
            if shutil.which(executable):
                run_validator(command, description)

    policy_path = root / "usr/share/polkit-1/actions/io.github.roghelper.policy"
    policy = ET.parse(policy_path).getroot()
    expected_actions = {
        "io.github.roghelper.cpu.control",
        "io.github.roghelper.battery.control",
        "io.github.roghelper.fans.control",
        "io.github.roghelper.lighting.control",
    }
    actions = policy.findall("./action")
    if {action.get("id") for action in actions} != expected_actions:
        fail("privileged PolicyKit action set is incorrect")
    for action in actions:
        defaults = action.find("./defaults")
        action_id = action.get("id") or "(missing id)"
        if defaults is None:
            fail(f"PolicyKit action has no defaults: {action_id}")
        if defaults.findtext("allow_any") != "no":
            fail(f"PolicyKit action permits arbitrary users: {action_id}")
        if defaults.findtext("allow_inactive") != "auth_admin":
            fail(f"PolicyKit inactive authorization is too broad: {action_id}")
        if defaults.findtext("allow_active") != "auth_admin":
            fail(f"PolicyKit active authorization is too broad: {action_id}")

    aura_rule = (root / "usr/lib/udev/rules.d/60-rog-helper-aura.rules").read_text(
        encoding="utf-8"
    )
    for required in (
        'ATTRS{idVendor}=="0b05"',
        'ATTRS{idProduct}=="19b6"',
        'ENV{ID_USB_INTERFACE_NUM}=="00"',
        'SYMLINK+="rog-helper-aura"',
    ):
        if required not in aura_rule:
            fail(f"Aura udev identity/alias rule missing: {required}")
    for forbidden in (
        'TAG+="uaccess"',
        "MODE=",
        "GROUP=",
        "RUN=",
        "SYSTEMD_WANTS=",
    ):
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
    deb_source: Path | None = None
    if source.is_dir():
        context = nullcontext(source)
    elif source.is_file() and source.suffix == ".deb":
        deb_source = source
        temporary = tempfile.TemporaryDirectory(prefix="rog-helper-deb-check-")

        class ExtractedPackage:
            def __enter__(self) -> Path:
                root = Path(temporary.__enter__())
                subprocess.run(
                    ["dpkg-deb", "--extract", str(source), str(root)], check=True
                )
                subprocess.run(
                    ["dpkg-deb", "--control", str(source), str(root / "DEBIAN")],
                    check=True,
                )
                return root

            def __exit__(self, *args: object) -> None:
                temporary.__exit__(*args)

        context = ExtractedPackage()
    else:
        print(f"not a staged tree or Debian package: {source}", file=sys.stderr)
        return 2

    try:
        if deb_source is not None:
            validate_archive_ownership(deb_source, "--fsys-tarfile")
            validate_archive_ownership(deb_source, "--ctrl-tarfile")
        with context as root:
            validate(root, privileged_only)
    except (
        AssertionError,
        ET.ParseError,
        OSError,
        subprocess.CalledProcessError,
        tarfile.TarError,
    ) as error:
        print(f"package payload validation failed: {error}", file=sys.stderr)
        return 1

    print(f"package payload OK: {source}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
