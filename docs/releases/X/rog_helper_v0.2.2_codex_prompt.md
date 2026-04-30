# Codex Prompt: Prepare rog-helper v0.2.2 Release

Repository: https://github.com/UdayaSri0/g-helper-linux

Current release: v0.2.1  
Target release: v0.2.2

You are working inside the existing `g-helper-linux` repository. Your task is to fully prepare the next release, `v0.2.2`, by inspecting the whole project, updating all version references, refreshing documentation, verifying package generation, preparing release notes, and making sure the GitHub release workflow can publish all installable Linux packages.

Do not invent features. Only document and advertise features that are actually implemented in the source code.

Do not push to GitHub automatically unless I explicitly ask you to do so.

---

## 1. Inspect the Current Repository

First, inspect the repository state:

```bash
git status --short
git branch --show-current
git tag --list "v0.2.*" --sort=-v:refname | head -20
```

Then search for all old version references:

```bash
rg -n "0\.2\.1|v0\.2\.1|0\.2\.0|v0\.2\.0|0\.16|v0\.16|rog-helper-v0\.2\.1|rog-helper_0\.2\.1|rog-helper-0\.2\.1" .
```

Also inspect release, package, workflow, and metadata files:

```bash
rg -n "version|Version|VERSION|release|Release|tag|AppImage|deb|rpm|tar\.xz|SHA256|desktop|metainfo|appdata|icon" .
find . -maxdepth 4 -type f | sort
```

Inspect at minimum:

- `Cargo.toml`
- `Cargo.lock`
- `crates/*/Cargo.toml`
- `crates/rog-ui/src/main.rs`
- `README.md`
- `CHANGELOG.md` if present
- `docs/`
- `packaging/`
- `scripts/`
- `.github/workflows/`
- desktop file
- AppImage packaging files
- Debian packaging files
- RPM packaging files
- icon/logo assets

---

## 2. Create a Release Branch

Create a release branch:

```bash
git checkout -b release/v0.2.2
```

If the branch already exists, switch to it:

```bash
git checkout release/v0.2.2
```

---

## 3. Update Version to v0.2.2

Update every correct release-related version reference from:

- `0.2.1` to `0.2.2`
- `v0.2.1` to `v0.2.2`

Update all relevant places:

- workspace version in `Cargo.toml`
- crate package versions if explicitly defined
- UI About page version display
- CLI version output if present
- AppImage name/version
- Debian package version
- RPM package version
- tarball name/version
- checksum file names
- release workflow tag checks
- README version text
- changelog
- release notes
- packaging metadata
- desktop/AppStream metadata if present

Important:

- Use Cargo metadata such as `env!("CARGO_PKG_VERSION")` wherever possible.
- Do not hardcode `0.2.2` in multiple Rust files unless there is no better option.
- Remove or correct stale version text such as `v0.16` if it represents the old application release version.
- Do not blindly replace unrelated historical examples.

After editing, run:

```bash
cargo check --workspace
```

---

## 4. Refresh Documentation

Update documentation so it accurately reflects the current v0.2.2 release.

Review and update:

- `README.md`
- `docs/RELEASE_CHECKLIST.md`
- `docs/BUILD.md`
- `docs/TROUBLESHOOTING.md`
- `docs/HARDWARE_SUPPORT.md`
- `docs/ROADMAP.md`
- `docs/GUI_SPEC.md`
- `docs/UI_PAGES.md`
- `docs/FEATURE_MATRIX.md`
- `docs/PROVIDER_MATRIX.md`
- `docs/DBUS_API.md`
- `docs/PERMISSIONS.md`
- `docs/DEVELOPMENT.md`

The docs must clearly state:

- Current target release is `v0.2.2`.
- The project is early-stage but already usable.
- Implemented features include:
  - GTK/libadwaita UI
  - user-session daemon
  - diagnostics CLI
  - telemetry dashboard
  - CPU telemetry and basic CPU controls
  - GPU mode support through `supergfxd`
  - ASUS profile and charge-limit support through `asusd`
  - keyboard backlight through sysfs
  - battery/power telemetry
  - RAM/swap telemetry
  - diagnostics page
  - tray menu
- Missing or incomplete features include:
  - fan curves
  - Aura/RGB runtime backend
  - policy automation
  - persistent configuration
  - typed DBus payloads
  - complete hardware compatibility matrix

Do not claim broad ASUS laptop support unless real hardware validation records exist.

Update `docs/HARDWARE_SUPPORT.md` with a `v0.2.2 Hardware Validation` section. If no new testing was done, clearly say:

```markdown
No new hardware validation logs were captured for v0.2.2. Hardware support remains provisional and depends on `asusd`, `supergfxd`, UPower, hwmon, sysfs permissions, and the target desktop environment.
```

---

## 5. Create Detailed Release Notes

Create this file:

```text
docs/releases/v0.2.2.md
```

Create the folder if needed.

The release note must include:

```markdown
# rog-helper v0.2.2

## Release Type

Maintenance, documentation, packaging, and release-readiness update.

## Highlights

- Version metadata aligned to `v0.2.2`.
- Release documentation refreshed for the current implementation.
- Packaging workflow reviewed for AppImage, Debian, RPM, tarball, raw binaries, and checksums.
- GitHub Actions release workflow checked for tag/version alignment.
- Known limitations documented clearly.

## Downloads

Expected release assets:

- `rog-helper`
- `rog-helperd`
- `rog-helper-ui`
- `rog-helper-v0.2.2-x86_64.AppImage`
- `rog-helper_0.2.2_amd64.deb`
- `rog-helper-0.2.2-1.x86_64.rpm`
- `rog-helper-0.2.2-linux-x86_64.tar.xz`
- `rog-helper-0.2.2-SHA256SUMS.txt`
- `rog-helper-0.2.2-RPM-SHA256SUMS.txt` if the RPM workflow still uses a separate checksum file

## Installation

### AppImage

```bash
chmod +x rog-helper-v0.2.2-x86_64.AppImage
./rog-helper-v0.2.2-x86_64.AppImage
```

### Debian / Ubuntu / Linux Mint

```bash
sudo apt install ./rog-helper_0.2.2_amd64.deb
```

### Fedora / RPM-based distributions

```bash
sudo dnf install ./rog-helper-0.2.2-1.x86_64.rpm
```

### Tarball

```bash
tar -xf rog-helper-0.2.2-linux-x86_64.tar.xz
```

## What Changed

Use actual git history only:

```bash
git log --oneline v0.2.1..HEAD
git diff --stat v0.2.1..HEAD
```

Summarise only real changes.

## Validation

Include the exact commands run and the result:

- `cargo fmt --all -- --check`
- `cargo build --workspace`
- `cargo test --workspace`
- `cargo clippy --workspace --all-targets -- -D warnings`
- `bash scripts/build-release.sh`
- checksum verification

## Known Limitations

- Fan-curve editing is not implemented end-to-end yet.
- Aura/RGB runtime backend is not implemented yet.
- Policy automation is not wired into runtime yet.
- Persistent user configuration is not implemented yet.
- Hardware support matrix is still incomplete.
- Some features depend on `asusd`, `supergfxd`, UPower, writable sysfs, and desktop tray support.

## Full Changelog

https://github.com/UdayaSri0/g-helper-linux/compare/v0.2.1...v0.2.2
```

Also update `CHANGELOG.md`. If it does not exist, create it and add a top section for `v0.2.2`.

---

## 6. Verify Packaging

The release must generate installable packages, not only GitHub source archives.

Verify or fix the packaging flow so it can produce:

- raw binaries:
  - `rog-helper`
  - `rog-helperd`
  - `rog-helper-ui`
- AppImage:
  - `rog-helper-v0.2.2-x86_64.AppImage`
- Debian package:
  - `rog-helper_0.2.2_amd64.deb`
- RPM package:
  - `rog-helper-0.2.2-1.x86_64.rpm`
- tarball:
  - `rog-helper-0.2.2-linux-x86_64.tar.xz`
- checksums:
  - `rog-helper-0.2.2-SHA256SUMS.txt`

Packaging must include:

- application binaries
- `.desktop` file
- app icon
- systemd user service for `rog-helperd`
- README or install notes
- license file if present

If `assets/logo.png` exists, use it as the source icon and install it into standard icon paths, for example:

```text
/usr/share/icons/hicolor/256x256/apps/rog-helper.png
```

The desktop file should use:

```text
Icon=rog-helper
```

Make sure the icon is actually included in the packages.

---

## 7. Create or Update Release Build Script

If a release build script already exists, update it.

If not, create:

```text
scripts/build-release.sh
```

The script must:

- use `set -euo pipefail`
- clean and recreate `dist/`
- build release binaries
- copy raw binaries to `dist/`
- build AppImage if tooling is available
- build Debian package
- build RPM package if tooling is available
- build tarball
- generate SHA256 checksums
- verify expected artifacts exist
- print final artifact list with file sizes

Expected output files:

```text
dist/rog-helper
dist/rog-helperd
dist/rog-helper-ui
dist/rog-helper-v0.2.2-x86_64.AppImage
dist/rog-helper_0.2.2_amd64.deb
dist/rog-helper-0.2.2-1.x86_64.rpm
dist/rog-helper-0.2.2-linux-x86_64.tar.xz
dist/rog-helper-0.2.2-SHA256SUMS.txt
```

If AppImage or RPM tooling is missing locally, the script must print a clear message explaining the missing dependency.

The script must not silently succeed when important artifacts are missing.

---

## 8. Update GitHub Actions Release Workflow

Inspect `.github/workflows/`.

Update or create a release workflow that runs on tags:

```yaml
on:
  push:
    tags:
      - "v*"
```

The workflow must:

1. Checkout source
2. Install Rust stable
3. Install Linux build dependencies:
   - `build-essential`
   - `pkg-config`
   - `libgtk-4-dev`
   - `libadwaita-1-dev`
   - Debian packaging tools
   - RPM packaging tools
   - AppImage tooling if required
4. Run validation:
   ```bash
   cargo fmt --all -- --check
   cargo clippy --workspace --all-targets -- -D warnings
   cargo test --workspace
   cargo build --workspace --release
   ```
5. Validate tag matches Cargo workspace version:
   ```bash
   VERSION="$(python3 -c 'from pathlib import Path; import tomllib; print(tomllib.loads(Path("Cargo.toml").read_text())["workspace"]["package"]["version"])')"
   test "v${VERSION}" = "${GITHUB_REF_NAME}"
   ```
6. Run:
   ```bash
   bash scripts/build-release.sh
   ```
7. Upload all `dist/` artifacts to the GitHub release.
8. Use `docs/releases/v0.2.2.md` as the release body for the `v0.2.2` release.

The workflow must fail if the tag is `v0.2.2` but the workspace version is not `0.2.2`.

---

## 9. Run Validation

Run these commands and fix all problems:

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
bash scripts/build-release.sh
ls -lah dist/
sha256sum -c dist/rog-helper-0.2.2-SHA256SUMS.txt
```

Also run these if the tools are available:

```bash
desktop-file-validate packaging/desktop/rog-helper.desktop || true
appstreamcli validate --no-net packaging/metainfo/*.xml || true
dpkg-deb --info dist/rog-helper_0.2.2_amd64.deb
dpkg-deb --contents dist/rog-helper_0.2.2_amd64.deb
rpm -qip dist/rog-helper-0.2.2-1.x86_64.rpm || true
rpm -qlp dist/rog-helper-0.2.2-1.x86_64.rpm || true
```

If a tool is missing, do not hide that. Mention it in the final report.

---

## 10. Prepare Git Commands

After validation passes, prepare these commands for me.

Do not run push commands unless I explicitly ask.

```bash
git status --short
git diff --stat
git add .
git commit -m "chore(release): prepare v0.2.2"
git tag -a v0.2.2 -F docs/releases/v0.2.2.md
git push origin release/v0.2.2
git push origin v0.2.2
```

Also prepare this GitHub CLI command:

```bash
gh release create v0.2.2 dist/* \
  --title "rog-helper v0.2.2" \
  --notes-file docs/releases/v0.2.2.md
```

---

## 11. Final Report

When finished, give me a clear final report with:

1. Files changed
2. Version references updated
3. Documentation updated
4. Packaging changes
5. GitHub Actions changes
6. Release artifacts generated
7. Validation commands run
8. Pass/fail status of each command
9. Missing local tools, if any
10. Known limitations still remaining
11. Exact commands I should run next to commit, tag, push, and publish

Do not say the release is ready unless validation passed and release artifacts were generated successfully.
