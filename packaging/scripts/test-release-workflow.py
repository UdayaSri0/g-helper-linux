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
assert "  publish-release:\n    needs: [verify-release-assets, verify-release-environment]" in text
assert "  verify-release-assets:\n    needs: [linux-release, fedora-rpm]" in text
assert "  verify-release-environment:\n    needs: verify-release-assets" in text
assert "    environment:\n      name: release-publish" in text
# The pinned action only extracts ID-selected artifacts directly into path
# when merge-multiple is true, even when exactly one ID is selected.
downloads = re.findall(r"^      - uses: actions/download-artifact@[^\n]+\n"
                       r"        with:\n((?:          [^\n]+\n)+)", text, re.MULTILINE)
assert len(downloads) == 3
for download, (source, destination) in zip(downloads, (
    ("linux-release.outputs.artifact_id", "linux"),
    ("fedora-rpm.outputs.artifact_id", "rpm"),
    ("verify-release-assets.outputs.bundle_artifact_id", "verified"),
), strict=True):
    assert dict(line.strip().split(": ", 1) for line in download.splitlines()) == {
        "artifact-ids": "${{ needs." + source + " || 'invalid' }}",
        "merge-multiple": "true",
        "path": "${{ runner.temp }}/" + destination,
    }, destination
assert text.count("persist-credentials: false") == 5
assert text.count('if [[ "$GIT_REF" != refs/tags/v* ]]') == 2
assert "if: startsWith(github.ref, 'refs/tags/v')" in text
assert '--notes-file "$RUNNER_TEMP/release-body.md"' in text
assert "Verified build provenance" in text and "COMMIT: ${{ github.sha }}" in text
assert "check-release-environment.py" in text
assert "secrets." not in text
assert text.count('check-release-ref.sh "$TAG" "$COMMIT"') == 2
assert 'scripts/test-cargo-audit.sh' in text

publisher_start = lines.index("  publish-release:")
build_jobs = "\n".join(lines[:publisher_start])
assert "contents: write" not in build_jobs
publisher = "\n".join(lines[publisher_start:])
assert "actions/download-artifact@" in publisher
assert publisher.count("gh release create") == 1
assert "cargo build" not in publisher and "build-release-assets.sh" not in publisher
assert "--draft --verify-tag" in publisher
assert "gh release edit" not in publisher

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
v041_notes = (docs_root / "releases/v0.4.1.md").read_text()
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
assert "rog-helper_0.4.1_amd64.deb" in v041_notes
assert "grep -E '^[[:xdigit:]]{64}  rog-helper_0\\.4\\.1_amd64\\.deb$'" in v041_notes
assert 'test "$(wc -l < "$asset.sha256")" -eq 1' in v041_notes
assert 'sha256sum --check "$asset.sha256"' in v041_notes
assert "independently authenticated digital signature" in v041_notes
assert "rog-helper-0.4.0-linux-x86_64.tar.xz" in build_guide
assert "curl -fLO https://github.com/UdayaSri0/g-helper-linux/releases/download/v0.4.0/rog-helper-0.4.0-SHA256SUMS.txt" in build_guide
assert 'sha256sum --check "$asset.sha256"' in build_guide
print("Release download and checksum documentation tests passed")

ci_workflow = workflow.parent / "ci.yml"
ci_lines = ci_workflow.read_text().splitlines()
ci_text = "\n".join(ci_lines)
ci_action_lines = [line.strip() for line in ci_lines if re.match(r"\s*uses:", line)]
for line in ci_action_lines:
    assert re.fullmatch(r"uses: [^@]+@[0-9a-f]{40} # .+", line), line
assert "permissions:\n  contents: read" in ci_text
assert "contents: write" not in ci_text
assert ci_text.count("persist-credentials: false") == 3
assert ci_text.count("retention-days: 14") == 2
assert ci_text.count("if: github.event_name == 'push' && !startsWith(github.ref, 'refs/tags/')") == 2
assert ci_text.count("actions/upload-artifact@ea165f8d65b6e75b540449e92b4886f43607fa02 # v4") == 3
for inventory in ("candidate-artifact-inventory.txt", "sha256sum --check \"$manifest\""):
    assert ci_text.count(inventory) >= 2, inventory
assert ci_text.index("Verify and describe AppImage candidate") < ci_text.index("Retain candidate AppImage artifact")
assert ci_text.index("Verify and describe Fedora RPM candidate") < ci_text.index("Retain candidate Fedora RPM artifact")
assert "secrets." not in ci_text
print("Candidate artifact retention permission, provenance, and checksum tests passed")
