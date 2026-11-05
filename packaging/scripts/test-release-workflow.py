#!/usr/bin/env python3
from pathlib import Path
import re


workflow = Path(__file__).resolve().parents[2] / ".github/workflows/release.yml"
lines = workflow.read_text().splitlines()
text = "\n".join(lines)

action_lines = [line.strip() for line in lines if re.match(r"\s*uses:", line)]
assert action_lines, "release workflow must use pinned actions"
for line in action_lines:
    assert re.fullmatch(r"uses: [^@]+@[0-9a-f]{40} # .+", line), line

assert text.count("contents: write") == 1, "only the publisher may have write access"
assert "    permissions:\n      contents: write\n    steps:" in text
assert "  publish-release:\n    needs: [linux-release, fedora-rpm]" in text
assert text.count("name: release-packages-${{ github.ref_name }}") == 2
assert text.count("name: fedora-rpm-${{ github.ref_name }}") == 2
assert text.count("persist-credentials: false") == 3
assert text.count('if [[ "$GIT_REF" != refs/tags/v* ]]') == 2
assert "if: startsWith(github.ref, 'refs/tags/v')" in text

publisher_start = lines.index("  publish-release:")
build_jobs = "\n".join(lines[:publisher_start])
assert "contents: write" not in build_jobs
publisher = "\n".join(lines[publisher_start:])
assert "actions/download-artifact@" in publisher
assert "softprops/action-gh-release@" in publisher
assert "cargo build" not in publisher and "packaging/scripts/" not in publisher

print("Release workflow permission and action-pin tests passed")

build_script = workflow.parents[2] / "packaging/scripts/build-appimage.sh"
build = build_script.read_text()
for pin_name in (
    "LINUXDEPLOY_SHA256",
    "GTK_PLUGIN_SHA256",
    "APPIMAGETOOL_SHA256",
    "APPIMAGE_RUNTIME_SHA256",
):
    assert re.search(rf'^{pin_name}="[0-9a-f]{{64}}"$', build, re.MULTILINE), pin_name
assert build.count("download_verified_tool ") == 4
assert "download_tool " not in build
assert "--runtime-file \"$TOOLS_DIR/runtime-x86_64\"" in build
assert "APPIMAGE_PLUGIN_URL" not in build

print("AppImage tool pin coverage tests passed")

docs_root = workflow.parents[2] / "docs"
readme = (workflow.parents[2] / "README.md").read_text()
release_notes = (docs_root / "releases/v0.4.0.md").read_text()
build_guide = (docs_root / "BUILD.md").read_text()
for artifact, manifest in (
    ("rog-helper_0.4.0_amd64.deb", "rog-helper-0.4.0-SHA256SUMS.txt"),
    ("rog-helper-0.4.0-1.x86_64.rpm", "rog-helper-0.4.0-RPM-SHA256SUMS.txt"),
    ("rog-helper-v0.4.0-x86_64.AppImage", "rog-helper-0.4.0-SHA256SUMS.txt"),
):
    assert f"curl -fLO https://github.com/UdayaSri0/g-helper-linux/releases/download/v0.4.0/{artifact}" in readme
    assert f"curl -fLO https://github.com/UdayaSri0/g-helper-linux/releases/download/v0.4.0/{manifest}" in readme
    assert "grep -E" in readme and 'sha256sum --check "$asset.sha256"' in readme
    assert "test -s \"$manifest\"" in readme and "test -s \"$asset\"" in readme
assert "rog-helper_0.4.0_amd64.deb" in release_notes
assert "curl -fLO https://github.com/UdayaSri0/g-helper-linux/releases/download/v0.4.0/rog-helper-0.4.0-SHA256SUMS.txt" in release_notes
assert 'sha256sum -c rog-helper-0.4.0-SHA256SUMS.txt --ignore-missing' not in readme + release_notes
assert "rog-helper-0.4.0-linux-x86_64.tar.xz" in build_guide
assert "curl -fLO https://github.com/UdayaSri0/g-helper-linux/releases/download/v0.4.0/rog-helper-0.4.0-SHA256SUMS.txt" in build_guide
assert 'sha256sum --check "$asset.sha256"' in build_guide
print("Release download and checksum documentation tests passed")
