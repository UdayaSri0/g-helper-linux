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
    require(provided == set(expected_binaries.values()), "AppStream binary list drift")

    systemd = text("packaging/systemd-user/rog-helperd.service")
    require("ExecStart=rog-helperd" in systemd, "systemd daemon binary drift")
    dbus = text("packaging/dbus-session/io.github.roghelper.Daemon.service")
    require("Name=io.github.roghelper.Daemon" in dbus, "DBus service name drift")
    require("Exec=rog-helperd" in dbus, "DBus daemon binary drift")

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
