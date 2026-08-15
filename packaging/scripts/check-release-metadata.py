#!/usr/bin/env python3
"""Dependency-free release metadata and packaged-asset consistency check."""

from __future__ import annotations

import re
import struct
import sys
import tomllib
import xml.etree.ElementTree as ET
from pathlib import Path


ROOT = Path(__file__).resolve().parents[2]
APP_ID = "io.github.roghelper.UI"
REPOSITORY = "https://github.com/UdayaSri0/g-helper-linux"


def require(condition: bool, message: str) -> None:
    if not condition:
        raise AssertionError(message)


def text(path: str) -> str:
    target = ROOT / path
    require(target.is_file(), f"missing required file: {path}")
    return target.read_text(encoding="utf-8")


def parse_desktop(contents: str) -> dict[str, str]:
    values: dict[str, str] = {}
    for line in contents.splitlines():
        if line and not line.startswith(("#", "[")) and "=" in line:
            key, value = line.split("=", 1)
            values[key] = value
    return values


def png_size(path: Path) -> tuple[int, int]:
    with path.open("rb") as stream:
        header = stream.read(24)
    require(header[:8] == b"\x89PNG\r\n\x1a\n", f"not a PNG: {path}")
    return struct.unpack(">II", header[16:24])


def main() -> int:
    cargo = tomllib.loads(text("Cargo.toml"))
    package = cargo["workspace"]["package"]
    version = package["version"]
    require(package["license"] == "MIT OR Apache-2.0", "workspace license drift")
    require(package["repository"] == REPOSITORY, "workspace repository URL drift")
    require(package["homepage"] == REPOSITORY, "workspace homepage URL drift")
    require(bool(package["authors"]), "workspace authors must not be empty")

    expected_binaries = {
        "rog-cli": "rog-helper",
        "rog-daemon": "rog-helperd",
        "rog-privileged": "rog-helper-privileged",
        "rog-ui": "rog-helper-ui",
    }
    for manifest_path in sorted((ROOT / "crates").glob("*/Cargo.toml")):
        manifest = tomllib.loads(manifest_path.read_text(encoding="utf-8"))
        crate = manifest["package"]
        for inherited in ["version", "edition", "authors", "homepage", "license", "repository"]:
            require(crate.get(inherited, {}).get("workspace") is True, f"{manifest_path}: {inherited} must inherit workspace metadata")
        if manifest_path.parent.name in expected_binaries:
            require(manifest["bin"][0]["name"] == expected_binaries[manifest_path.parent.name], f"{manifest_path}: binary name drift")

    desktop = parse_desktop(text("packaging/desktop/rog-helper.desktop"))
    require(desktop.get("Exec") == "rog-helper-ui", "desktop Exec drift")
    require(desktop.get("TryExec") == "rog-helper-ui", "desktop TryExec drift")
    require(desktop.get("Icon") == "rog-helper", "desktop icon drift")
    require(desktop.get("StartupWMClass") == APP_ID, "desktop application ID drift")

    metainfo = ET.fromstring(text("packaging/metainfo/io.github.roghelper.UI.metainfo.xml"))
    require(metainfo.findtext("id") == APP_ID, "AppStream ID drift")
    require(metainfo.findtext("project_license") == package["license"], "AppStream license drift")
    require(metainfo.findtext("launchable") == "rog-helper.desktop", "AppStream desktop ID drift")
    require(metainfo.find("url[@type='homepage']").text == REPOSITORY, "AppStream repository URL drift")
    provided = {node.text for node in metainfo.findall("./provides/binary")}
    desktop_binaries = {name for name in expected_binaries.values() if name != "rog-helper-privileged"}
    require(provided == desktop_binaries, "AppStream binary list drift")

    systemd = text("packaging/systemd-user/rog-helperd.service")
    require("ExecStart=rog-helperd" in systemd, "systemd daemon binary drift")
    dbus = text("packaging/dbus-session/io.github.roghelper.Daemon.service")
    require("Name=io.github.roghelper.Daemon" in dbus, "DBus service name drift")
    require("Exec=rog-helperd" in dbus, "DBus daemon binary drift")

    privileged_dbus = text("packaging/dbus-system/io.github.roghelper.Privileged.service")
    require("Name=io.github.roghelper.Privileged" in privileged_dbus, "privileged DBus service name drift")
    require("SystemdService=rog-helper-privileged.service" in privileged_dbus, "privileged DBus/systemd activation drift")
    privileged_bus_policy = ET.fromstring(text("packaging/dbus-system/io.github.roghelper.Privileged.conf"))
    allow_rules = privileged_bus_policy.findall("./policy/allow")
    require(any(rule.get("own") == "io.github.roghelper.Privileged" for rule in allow_rules), "privileged DBus ownership policy missing")
    require(any(rule.get("send_destination") == "io.github.roghelper.Privileged" for rule in allow_rules), "privileged DBus send policy missing")

    privileged_systemd = text("packaging/systemd-system/rog-helper-privileged.service")
    require("Type=dbus" in privileged_systemd, "privileged service must use Type=dbus")
    require("BusName=io.github.roghelper.Privileged" in privileged_systemd, "privileged systemd bus name drift")
    require("User=root" in privileged_systemd, "privileged service user drift")
    for directive in [
        "NoNewPrivileges=yes",
        "PrivateTmp=yes",
        "PrivateDevices=yes",
        "ProtectHome=yes",
        "ProtectSystem=strict",
        "ProtectKernelModules=yes",
        "ProtectControlGroups=yes",
        "ProtectKernelLogs=yes",
        "ProtectClock=yes",
        "ProtectHostname=yes",
        "ProtectProc=invisible",
        "ProcSubset=pid",
        "RestrictAddressFamilies=AF_UNIX",
        "RestrictNamespaces=yes",
        "LockPersonality=yes",
        "MemoryDenyWriteExecute=yes",
        "RestrictRealtime=yes",
        "RestrictSUIDSGID=yes",
        "SystemCallFilter=@system-service",
        "Restart=on-failure",
        "RuntimeDirectoryMode=0700",
        "UMask=0077",
    ]:
        require(directive in privileged_systemd, f"privileged service hardening drift: {directive}")
    require("ProtectKernelTunables=no" in privileged_systemd, "required sysfs-write exception is undocumented in the unit")

    package_common = text("packaging/scripts/package-common.sh")
    require("umask 0022" in package_common, "package payload modes must not inherit a permissive builder umask")
    require("dest_path.chmod(0o644)" in package_common, "rendered root-consumed metadata must be mode 0644")
    tarball_builder = text("packaging/scripts/build-tarball.sh")
    require(
        'install_privileged_integration "$BUNDLE_ROOT" "/usr/local/libexec/rog-helper-privileged"'
        in tarball_builder,
        "portable root helper activation must use an absolute executable path",
    )

    polkit = ET.fromstring(text("packaging/polkit/io.github.roghelper.policy"))
    actions = polkit.findall("./action")
    action_ids = {action.get("id") for action in actions}
    expected_actions = {
        "io.github.roghelper.cpu.control",
        "io.github.roghelper.battery.control",
        "io.github.roghelper.fans.control",
        "io.github.roghelper.lighting.control",
    }
    require(action_ids == expected_actions, "privileged PolicyKit action set drift")
    require("io.github.roghelper.root" not in action_ids, "generic root PolicyKit action is forbidden")
    for action in actions:
        action_id = action.get("id") or "(missing id)"
        require(bool((action.findtext("description") or "").strip()), f"{action_id}: missing PolicyKit description")
        require(bool((action.findtext("message") or "").strip()), f"{action_id}: missing PolicyKit message")
        defaults = action.find("defaults")
        require(defaults is not None, f"{action_id}: missing PolicyKit defaults")
        require(defaults.findtext("allow_any") == "no", f"{action_id}: arbitrary users must not be authorized")
        require(defaults.findtext("allow_inactive") == "auth_admin", f"{action_id}: inactive policy drift")
        require(defaults.findtext("allow_active") == "auth_admin", f"{action_id}: retained authorization is forbidden")

    resource_path = ROOT / "crates/rog-ui/resources/rog-ui.gresource.xml"
    resource_root = ET.parse(resource_path).getroot()
    resource_dir = resource_path.parent
    resources = resource_root.findall("./gresource/file")
    require(len(resources) >= 11, "UI resource bundle unexpectedly incomplete")
    for node in resources:
        require((resource_dir / (node.text or "")).resolve().is_file(), f"missing UI resource: {node.text}")

    master_logo = ROOT / "assets/logo.png"
    require(master_logo.is_file(), "missing master logo")
    for size in [16, 32, 48, 64, 128, 256, 512]:
        icon = ROOT / f"packaging/desktop/icons/hicolor/{size}x{size}/apps/rog-helper.png"
        require(icon.is_file(), f"missing packaged {size}px icon")
        require(png_size(icon) == (size, size), f"incorrect dimensions for {icon}")

    arch_srcinfo = text("packaging/arch/.SRCINFO")
    require(re.search(rf"^\s*pkgver = {re.escape(version)}$", arch_srcinfo, re.MULTILINE) is not None, "Arch version drift")
    for path in [
        "LICENSE-MIT",
        "LICENSE-APACHE",
        "packaging/debian/copyright",
        "packaging/debian/control.in",
        "packaging/rpm/rog-helper.spec.in",
        "packaging/flatpak/io.github.roghelper.UI.yml",
        "packaging/flatpak/cargo-sources.json",
        "packaging/flatpak/flathub.json",
    ]:
        text(path)

    scripts = sorted((ROOT / "packaging/scripts").glob("*.sh"))
    require(scripts, "no packaging scripts found")
    print(f"release metadata OK: version {version}, {len(resources)} embedded UI assets, {len(scripts)} packaging scripts")
    return 0


if __name__ == "__main__":
    try:
        raise SystemExit(main())
    except (AssertionError, KeyError, OSError, ET.ParseError, tomllib.TOMLDecodeError) as error:
        print(f"release metadata check failed: {error}", file=sys.stderr)
        raise SystemExit(1)
