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
