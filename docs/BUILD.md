# Build

This document covers the current build, run, and validation workflow for the repository as implemented today.

## Prerequisites

- Rust stable via `rustup`
- A Cargo version new enough to read lockfile v4
- GTK4 development packages
- libadwaita development packages
- `pkg-config`

The workspace does not currently pin a `rust-toolchain.toml`, and CI uses stable Rust.

## Ubuntu/Debian Dependencies

Example package install:

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libgtk-4-dev libadwaita-1-dev
```

Recommended Rust install or refresh:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup toolchain install stable
rustup default stable
cargo --version
rustc --version
```

## Build Commands

Build the full workspace:

```bash
cargo build --workspace
```

Build a specific binary crate:

```bash
cargo build -p rog-daemon
cargo build -p rog-ui
cargo build -p rog-cli
```

## Run Commands

### Daemon

Start the session daemon:

```bash
cargo run -p rog-daemon
```

### UI

Start the GTK/libadwaita UI:

```bash
cargo run -p rog-ui
```

### CLI

Run the diagnostics CLI:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- dbus --filter "asus|rog|supergfx|power|upower"
cargo run -p rog-cli -- sensors
cargo run -p rog-cli -- caps
```

## Validation Commands

The repository CI currently runs:

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

These commands are defined in `.github/workflows/ci.yml`.

Important note:

- `fmt`, `build`, tests, and packaging scripts are all part of current release readiness.
- The CI configuration expects clippy-clean builds as well.
- If clippy fails locally, compare against the current source and CI state rather than assuming the docs are wrong.

## Verify the Session DBus API

With `rog-helperd` running:

If `rg` is not installed, replace it with `grep -E`.

```bash
busctl --user list | rg -n "io\\.github\\.roghelper\\.Daemon"
busctl --user introspect io.github.roghelper.Daemon /io/github/roghelper/Daemon
busctl --user call io.github.roghelper.Daemon /io/github/roghelper/Daemon io.github.roghelper.Daemon1 GetTelemetry
```

The current daemon DBus identity is:

- bus name: `io.github.roghelper.Daemon`
- object path: `/io/github/roghelper/Daemon`
- interface: `io.github.roghelper.Daemon1`

See [DBUS_API.md](DBUS_API.md) for the current API surface.

## Install Locally

Install binaries into `~/.cargo/bin`:

```bash
cargo install --path crates/rog-daemon --bin rog-helperd --locked
cargo install --path crates/rog-ui --bin rog-helper-ui --locked
cargo install --path crates/rog-cli --bin rog-helper --locked
```

## Install From Release Artifacts

Tagged releases publish:

- `rog-helper_<version>_<arch>.deb`
- `rog-helper-v<version>-x86_64.AppImage`
- `rog-helper-<version>-linux-<arch>.tar.xz`
- direct `rog-helper`, `rog-helperd`, and `rog-helper-ui` binaries
- `rog-helper-<version>-SHA256SUMS.txt`

Debian or Ubuntu install:

```bash
sudo apt-get install ./rog-helper_<version>_<arch>.deb
```

Portable prefix install under `/usr/local`:

```bash
sudo tar --strip-components=1 -xf rog-helper-<version>-linux-<arch>.tar.xz -C /usr/local
sudo update-desktop-database /usr/local/share/applications || true
sudo gtk-update-icon-cache -q /usr/local/share/icons/hicolor || true
systemctl --user daemon-reload
systemctl --user enable --now rog-helperd
```

Notes:

- the tarball contains `bin/`, `share/`, and `lib/systemd/user/` content laid out for a prefix such as `/usr/local`
- direct binary assets are primarily for advanced user-local installs and the UI's safe direct-binary update path
- verify release assets with the published SHA256 file before installing them
- a future APT repository can be staged locally with `packaging/scripts/stage-apt-repo.sh`, but no signed public repository is live yet

## Optional Desktop Launcher Install

The repository ships a desktop entry, AppStream metadata, session DBus activation metadata, and a generated hicolor PNG icon set.

Source of truth:

- `assets/logo.png`

Desktop assets:

- `packaging/desktop/rog-helper.desktop`
- `packaging/metainfo/io.github.roghelper.UI.metainfo.xml`
- `packaging/dbus-session/io.github.roghelper.Daemon.service`
- `packaging/desktop/icons/hicolor/16x16/apps/rog-helper.png`
- `packaging/desktop/icons/hicolor/24x24/apps/rog-helper.png`
- `packaging/desktop/icons/hicolor/32x32/apps/rog-helper.png`
- `packaging/desktop/icons/hicolor/48x48/apps/rog-helper.png`
- `packaging/desktop/icons/hicolor/64x64/apps/rog-helper.png`
- `packaging/desktop/icons/hicolor/128x128/apps/rog-helper.png`
- `packaging/desktop/icons/hicolor/256x256/apps/rog-helper.png`
- `packaging/desktop/icons/hicolor/512x512/apps/rog-helper.png`

Refresh the generated icon set after changing `assets/logo.png`:

```bash
python3 packaging/scripts/generate_icons.py
```

Notes:

- the icon generator uses Python + Pillow (`PIL`)
- the generated PNG set is created on demand by the packaging scripts and CI
- packaging builds reuse the generated PNG set instead of the old placeholder SVG

Install them locally with:

```bash
mkdir -p ~/.local/share/applications
cp packaging/desktop/rog-helper.desktop ~/.local/share/applications/
for size in 16 24 32 48 64 128 256 512; do
  mkdir -p "$HOME/.local/share/icons/hicolor/${size}x${size}/apps"
  cp "packaging/desktop/icons/hicolor/${size}x${size}/apps/rog-helper.png" \
    "$HOME/.local/share/icons/hicolor/${size}x${size}/apps/"
done
```

Notes:

- the desktop entry uses `Exec=rog-helper-ui`, so the UI binary still needs to be installed on `PATH`
- the desktop entry uses `Icon=rog-helper`, so the icon theme files should be installed together with the `.desktop` file
- `StartupWMClass=io.github.roghelper.UI` matches the current GTK application ID used by the UI
- some desktop environments may require an icon cache refresh before the launcher icon appears

## Packaging Helpers

Build a `.deb` staging package:

```bash
packaging/scripts/build-deb.sh
```

This builds release binaries, installs the desktop entry, AppStream metadata, session DBus activation, hicolor icons, all three binaries, license files, and the systemd user service into a Debian package under `dist/`.

Requirements:

- `dpkg-deb`
- `dpkg-shlibdeps`
- `desktop-file-validate` if desktop validation is desired
- `appstreamcli` if AppStream validation is desired
- `python3-pil` for generated icons

Build a prefix-friendly Linux tarball:

```bash
packaging/scripts/build-tarball.sh
```

This emits a `rog-helper-<version>-linux-<arch>.tar.xz` bundle in `dist/` with:

- `bin/rog-helper-ui`
- `bin/rog-helperd`
- `bin/rog-helper`
- `share/applications/rog-helper.desktop`
- `share/dbus-1/services/io.github.roghelper.Daemon.service`
- `share/metainfo/io.github.roghelper.UI.metainfo.xml`
- `share/icons/hicolor/...`
- `lib/systemd/user/rog-helperd.service`

Build the full tagged-release asset set:

```bash
packaging/scripts/build-release-assets.sh
```

This emits direct binaries, the `.deb`, the AppImage, the Linux tarball, and `SHA256` checksums under `dist/`.

Stage a future APT repository preview from built `.deb` files:

```bash
packaging/scripts/stage-apt-repo.sh dist dist/apt-repo-preview
```

This creates an unsigned Debian-family repository layout with:

- `pool/`
- `dists/stable/main/binary-amd64/Packages`
- `dists/stable/main/binary-amd64/Packages.gz`
- `dists/stable/Release`

It is a staging helper only; publishing a real repository still requires hosting and GPG signing.

Build the portable AppImage bundle:

```bash
packaging/scripts/build-appimage.sh
```

This emits `rog-helper-v<version>-x86_64.AppImage` in `dist/` and leaves the staged `dist/AppDir/` tree behind for inspection.

Current AppImage notes:

- the script downloads pinned `linuxdeploy`, `linuxdeploy-plugin-appimage`, and `linuxdeploy-plugin-gtk` helpers into a local cache directory if they are missing
- the AppImage bundles the UI, daemon, CLI, desktop launcher, hicolor icons, AppStream metadata, session D-Bus activation file, and the user-service unit
- the AppImage does not install menu integration or enable the user service on the host system
- the build currently targets `x86_64` only

## systemd --user

The repository includes a user service file at `packaging/systemd-user/rog-helperd.service`.

Install and enable it with:

```bash
mkdir -p ~/.config/systemd/user
cp packaging/systemd-user/rog-helperd.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now rog-helperd
systemctl --user status rog-helperd --no-pager
```

Important note:

- The unit uses `ExecStart=rog-helperd`, so it expects the binary to already be installed on `PATH`.
- The unit also sets a custom `PATH` that includes `%h/.cargo/bin` and `%h/.local/bin`.
- Debian packages render the installed unit with an absolute `ExecStart=/usr/bin/rog-helperd`.
- Release packages also install `io.github.roghelper.Daemon.service`, so opening the desktop app can activate the daemon on the session bus even when the user service is not yet enabled.

## Runtime Dependency Notes

The project can be built without ASUS-specific services, but real runtime behavior depends on them:

- `UPower`
  - Expected for battery and power-source telemetry
- `asusd`
  - Required for ASUS platform profile control
  - Required for ASUS battery charge-limit control
- `supergfxd`
  - Required for GPU mode control
- writable sysfs
  - Required for some CPU writes
  - Required for keyboard backlight brightness writes when using the sysfs backend

If these are missing or inaccessible, the application should still run, but affected features will be unavailable or read-only.

## Tray Support Notes

The tray implementation uses StatusNotifierItem through `ksni`.

On desktop environments that do not expose tray icons by default, especially GNOME, the tray may not be visible unless an AppIndicator/SNI extension is enabled.

The UI is designed to continue without a tray if SNI support is unavailable.

## Permission Notes

The UI and daemon are unprivileged by design.

That means:

- system DBus writes can still fail if the relevant service rejects them
- sysfs writes can still fail if the current user lacks write access
- the application may expose a feature as readable but not writable

Common examples:

- keyboard backlight brightness is visible but read-only
- CPU controls are visible but read-only, with per-control diagnostics showing the blocked sysfs paths
- `asusd` or `supergfxd` controls are missing because the service is not installed or not reachable

See [PERMISSIONS.md](PERMISSIONS.md) and [TROUBLESHOOTING.md](TROUBLESHOOTING.md) for more detail.
