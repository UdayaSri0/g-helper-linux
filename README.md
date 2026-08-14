# rog-helper

Linux-native control app for ASUS ROG laptops, built as a Rust workspace with a GTK/libadwaita UI, a user-session daemon, a provider layer for DBus/sysfs/procfs integration, and a diagnostics CLI.

This repository was reviewed and its markdown docs were refreshed against the current source code. When documentation and source disagree in the future, prefer the implementation in `crates/` and update the docs.

## Overview

`rog-helper` is designed to provide a single Linux-native control surface for ASUS laptop features that are otherwise split across multiple services and system interfaces. Packaged installs also include an optional, PolicyKit-gated root helper for a small set of typed hardware writes; the UI and session daemon remain unprivileged.

The current codebase implements:

- A GTK4/libadwaita desktop application with tray support via `ksni`
- A session-DBus daemon (`rog-helperd`) that owns current state and control actions
- Provider modules for `asusd`, `supergfxd`, `UPower`, `hwmon`, CPU sysfs, keyboard backlight sysfs, battery sysfs, memory telemetry, DBus diagnostics, and setup-readiness checks
- A CLI (`rog-helper`) for diagnostics and environment inspection

The project is clearly beyond an initial scaffold, but it is still an early implementation. Several core features are working today, while other planned features are still missing or only partially modeled.

## Architecture Summary

The current runtime architecture is:

```text
UI / tray
  -> session DBus (`io.github.roghelper.Daemon`)
  -> rog-helperd
  -> providers / system DBus (`asusd`, `supergfxd`, `UPower`)
  -> direct user-writable sysfs when safe
  -> optional system DBus + PolicyKit -> rog-helper-privileged -> approved sysfs ABI
```

High-level responsibilities:

- `rog-ui`: unprivileged GTK/libadwaita UI and tray
- `rog-daemon`: user-session daemon, state owner, telemetry aggregator, and control router
- `rog-providers`: DBus/sysfs/procfs integration layer
- `rog-core`: shared domain model, validation, and policy types
- `rog-cli`: diagnostics and probing tool
- `rog-privileged`: on-demand root service with path-free CPU, verified fan, keyboard-brightness, and battery-threshold methods

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the current runtime architecture.

## Quick Start and Docs

Start with [docs/QUICK_START.md](docs/QUICK_START.md) for the shortest path to:

- running the daemon, UI, and diagnostics CLI from source
- finding the main source files by feature area
- choosing the right user or developer document

## Workspace Overview

- `crates/rog-core`
  - Shared domain types, validation, errors, and policy model
  - No hardware or DBus I/O
- `crates/rog-providers`
  - Provider modules for `asusd`, `supergfxd`, `UPower`, `hwmon`, CPU controls, keyboard backlight, power-supply sysfs, memory telemetry, `nvidia-smi`, and DBus inspection helpers
- `crates/rog-daemon`
  - `rog-helperd` session daemon and DBus service
- `crates/rog-ui`
  - `rog-helper-ui` GTK4/libadwaita desktop application and tray
- `crates/rog-cli`
  - `rog-helper` diagnostics CLI
- `crates/rog-privileged`
  - Optional root system service; callers never provide paths, commands, programs, or raw hardware payloads

Basic packaging assets are also included under `packaging/`:

- `packaging/systemd-user/rog-helperd.service`
- `packaging/systemd-system/rog-helper-privileged.service`
- `packaging/dbus-system/`
- `packaging/polkit/io.github.roghelper.policy`
- `packaging/desktop/rog-helper.desktop`
- `packaging/dbus-session/io.github.roghelper.Daemon.service`
- `packaging/metainfo/io.github.roghelper.UI.metainfo.xml`
- `packaging/arch/PKGBUILD`
- `packaging/arch/.SRCINFO`
- `packaging/arch/rog-helper.install`
- `packaging/flatpak/io.github.roghelper.UI.yml`
- `packaging/flatpak/cargo-sources.json`
- `packaging/flatpak/flathub.json`
- `packaging/flatpak/README.md`
- generated-on-demand hicolor PNG icon set under `packaging/desktop/icons/hicolor/`
- `packaging/scripts/build-deb.sh`
- `packaging/scripts/build-rpm.sh`
- `packaging/scripts/build-flatpak.sh`
- `packaging/scripts/stage-apt-repo.sh`
- `packaging/scripts/build-tarball.sh`
- `packaging/scripts/build-appimage.sh`
- `packaging/scripts/build-release-assets.sh`
- `packaging/scripts/generate_icons.py`

Branding source of truth:

- `assets/logo.png`

## Currently Implemented Features

Current source-backed features include:

- Telemetry dashboard with CPU temperature, GPU temperature, battery, power source, fans, and warnings
- CPU telemetry and generic Linux CPU controls
  - physical-core count and logical-thread count
  - per-logical-CPU / thread usage, frequency, policy, and online/offline state
  - turbo boost
  - power mode
  - governor
  - energy performance preference
  - min/max frequency limits
  - per-logical-CPU online/offline toggles
  - structured diagnostics for `available`, `unsupported`, `missing_backend`, `permission_denied`, and `temporarily_unavailable` write states
- GPU mode read/write through `supergfxd`
- ASUS performance profile read/write through `asusd`
- ASUS battery charge limit read/write through `asusd`
- Keyboard backlight brightness read/write through sysfs when permissions allow
- Capability-aware unavailable/read-only UX that keeps controls visible and explains common missing-backend or permission-blocked states
- Dedicated Setup & Access UI plus `rog-helper setup-check`, backed by live API verification and read-only permission probes
- Fan monitoring and safe fan controls for supported ASUS/Linux hardware
  - polished RPM monitoring dashboard with animated fan rotors, larger CPU/GPU gauges, and best-effort operating MHz display
  - best-effort dynamic RPM telemetry for 0..N fans
  - verified eight-point ASUS WMI fan curves with direct or PolicyKit-gated writes
  - Auto/BIOS restore and curve sync when the exact ASUS device identity and channel mapping are confirmed
  - generic PWM, RPM-target, and boost candidates remain disabled even when a file appears writable
- Battery, power, health, and time estimates from `UPower` with sysfs fallback for additional details
- RAM, swap, PSI, zram, zswap, and top memory process telemetry
- Diagnostics UI and diagnostics CLI
- About page update/support section with manual GitHub release checks, release-note preview, source/support links, and issue-reporting action
- Limited UI lifecycle preferences for close-to-tray behavior, launch-on-login autostart, and start-minimized-to-tray behavior
- Official `rog-helper` logo wired into desktop packaging, launcher metadata, tray/window icon naming, and packaging scripts
- Session DBus API for the UI and other local clients
- Optional typed privileged fallback with separate CPU, fan, lighting, and battery PolicyKit actions; direct/asusd/supergfxd routes remain preferred

See:

- [docs/FEATURE_MATRIX.md](docs/FEATURE_MATRIX.md)
- [docs/DBUS_API.md](docs/DBUS_API.md)
- [docs/DBUS_CONTRACT_MAP.md](docs/DBUS_CONTRACT_MAP.md)
- [docs/PROVIDER_MATRIX.md](docs/PROVIDER_MATRIX.md)
- [docs/UI_PAGES.md](docs/UI_PAGES.md)
- [docs/PRIVILEGED_SECURITY_REVIEW.md](docs/PRIVILEGED_SECURITY_REVIEW.md)

## Current Missing or Incomplete Features

Important gaps in the current implementation:

- Broader fan-control contracts beyond the verified ASUS WMI eight-point curve ABI
- Broader Aura/RGB lighting validation across ASUS models and asusd versions
- Live auto mode / policy automation integration
- Persistent hardware/control configuration and saved automation rules beyond the current UI lifecycle preferences
- Generated strongly typed external DBus payloads (the current backwards-compatible `a{sv}` API
  now shares internal key constants and decoding semantics)
- Complete tested hardware support matrix
- Broader cross-distro install validation, including wider AppImage runtime validation beyond the current Ubuntu-class release host

Some related domain types and traits already exist in `rog-core` and `rog-providers`, but they are not fully wired into runtime behavior yet.

Hardware support note:

- unless a real machine record has been added under the hardware-validation docs, treat support for that machine or scenario as untested rather than validated

## Quick Start

### Prerequisites

- Rust stable via `rustup`
- GTK4 development packages
- libadwaita development packages
- `pkg-config`

Ubuntu/Debian example:

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libgtk-4-dev libadwaita-1-dev
```

### Build

```bash
cargo build --workspace
```

### Run

Start the daemon first:

```bash
cargo run -p rog-daemon
```

Start the UI in another terminal:

```bash
cargo run -p rog-ui
```

Optional CLI diagnostics:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- dbus --filter "asus|rog|aura|kbd|keyboard|led|rgb|supergfx|power|upower"
cargo run -p rog-cli -- caps
cargo run -p rog-cli -- lighting-diagnostics
```

## Release Installs

Tagged releases now ship Linux release assets instead of source-only tags:

- `.deb` packages for Debian/Ubuntu-style installs
- `.rpm` packages for Fedora-family installs
- portable `x86_64` AppImages for direct download-and-run use
- prefix-friendly Linux tarballs for `/usr/local`
- direct `rog-helper`, `rog-helperd`, and `rog-helper-ui` binaries for advanced user-local installs and the UI updater
- SHA256 checksum files for every published asset

See [docs/BUILD.md](docs/BUILD.md) for the current install paths and packaging commands.

## Debian / Ubuntu / Mint Installation

Direct `.deb` install:

```bash
sha256sum -c rog-helper-0.2.2-SHA256SUMS.txt --ignore-missing
sudo apt install ./rog-helper_0.2.2_amd64.deb
```

Optional user-session daemon enablement:

```bash
systemctl --user daemon-reload
systemctl --user enable --now rog-helperd.service
```

The Debian-family package installs:

- `rog-helper-ui`, `rog-helperd`, and `rog-helper` under `/usr/bin`
- the desktop launcher under `/usr/share/applications`
- hicolor icons under `/usr/share/icons/hicolor`
- AppStream metadata under `/usr/share/metainfo`
- session D-Bus activation under `/usr/share/dbus-1/services`
- the user service under `/usr/lib/systemd/user`
- the root-owned `rog-helper-privileged` binary under `/usr/libexec`, plus its systemd, system-D-Bus, and PolicyKit integration

Remove cleanly with:

```bash
sudo apt remove rog-helper
systemctl --user daemon-reload
```

## Fedora Installation

Direct `.rpm` install:

```bash
sha256sum -c rog-helper-0.2.2-RPM-SHA256SUMS.txt --ignore-missing
sudo dnf install ./rog-helper-0.2.2-1.x86_64.rpm
```

Optional user-session daemon enablement:

```bash
systemctl --user daemon-reload
systemctl --user enable --now rog-helperd.service
```

The Fedora RPM installs:

- `rog-helper-ui`, `rog-helperd`, and `rog-helper` under `/usr/bin`
- the desktop launcher under `/usr/share/applications`
- hicolor icons under `/usr/share/icons/hicolor`
- AppStream metadata under `/usr/share/metainfo`
- session D-Bus activation under `/usr/share/dbus-1/services`
- the user service under `/usr/lib/systemd/user`

Remove cleanly with:

```bash
sudo dnf remove rog-helper
systemctl --user daemon-reload
```

## Arch Linux Installation

This repository ships a source-based `PKGBUILD` under `packaging/arch/`.

Build and install it directly from a repository checkout:

```bash
git clone https://github.com/UdayaSri0/g-helper-linux.git
cd g-helper-linux/packaging/arch
makepkg -si
```

Optional user-session daemon enablement:

```bash
systemctl --user daemon-reload
systemctl --user enable --now rog-helperd.service
```

The Arch package installs:

- `rog-helper-ui`, `rog-helperd`, and `rog-helper` under `/usr/bin`
- the desktop launcher under `/usr/share/applications`
- hicolor icons under `/usr/share/icons/hicolor`
- AppStream metadata under `/usr/share/metainfo`
- session D-Bus activation under `/usr/share/dbus-1/services`
- the user service under `/usr/lib/systemd/user`

If an AUR package repository is published later, package helpers such as `yay`
or `paru` can install it. This repository already includes AUR-ready
`PKGBUILD`, `.SRCINFO`, and install-hook metadata, but it does not publish to
the AUR automatically today.

## Flatpak Installation

This repository ships a local Flatpak manifest under `packaging/flatpak/`.

Build and install it from a repository checkout:

```bash
flatpak remote-add --if-not-exists flathub https://flathub.org/repo/flathub.flatpakrepo
ROG_HELPER_FLATPAK_INSTALL=1 packaging/scripts/build-flatpak.sh
flatpak run io.github.roghelper.UI
```

The Flatpak bundles:

- `rog-helper-ui`, `rog-helperd`, and `rog-helper` under `/app/bin`
- a Flatpak-specific desktop file and AppStream metadata under `/app/share`
- both `rog-helper` and `io.github.roghelper.UI` icon names under the hicolor theme
- a session D-Bus activation file for the bundled `rog-helperd`

Flatpak runtime notes:

- `rog-helperd` runs inside the sandbox and is activated on the session bus; the host `systemd --user` unit is not installed or enabled
- `UPower`, `asusd`, and `supergfxd` access is requested through specific system D-Bus permissions, but the host services still have to be installed and may still reject sandboxed callers
- direct sysfs writes are not available in Flatpak, so CPU tuning, per-core toggles, and keyboard-backlight writes remain unavailable or read-only
- procfs-backed per-process diagnostics are limited to processes visible inside the sandbox
- the `nvidia-smi` fallback is not expected to work inside the sandbox
- tray integration may still be limited on hosts that require extra StatusNotifier D-Bus ownership

If a Flathub publication is added later, the end-user install path would become
`flatpak install flathub io.github.roghelper.UI`, but that repository is not
live today.

### Future APT Repository

A signed APT repository is not live yet.

The repository is now future-ready for static hosting through the staging helper at `packaging/scripts/stage-apt-repo.sh`, which generates `pool/`, `Packages`, `Packages.gz`, and `Release` metadata for a preview repository.

When a signed repository is published later, the install flow will look like this:

```text
# placeholder only: do not use until a real repository URL and signing key are published
deb [signed-by=/usr/share/keyrings/rog-helper-archive-keyring.gpg] https://<future-host>/ stable main
sudo apt update
sudo apt install rog-helper
```

## AppImage Usage

Download the AppImage and verify it before first run:

```bash
sha256sum -c rog-helper-0.2.2-SHA256SUMS.txt --ignore-missing
chmod +x rog-helper-v0.2.2-x86_64.AppImage
./rog-helper-v0.2.2-x86_64.AppImage
```

The AppImage bundles:

- `rog-helper-ui`
- `rog-helperd`
- `rog-helper`
- the desktop launcher, icon assets, AppStream metadata, and session D-Bus activation files needed inside the portable bundle

Important AppImage notes:

- the AppImage does not install itself into the system menu or enable desktop integration automatically
- the bundled `rog-helperd.service` unit is included for reference, but it is not installed or enabled by running the AppImage
- the AppImage updates the session environment so the bundled D-Bus activation file can find `rog-helperd`, but if you want persistent login-session integration, prefer the `.deb` package

## Build and Run Basics

Useful commands for local development:

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Important note: the repository CI expects `fmt`, `build`, `clippy`, tests, and packaging smoke tests to pass. If local results differ, compare against `.github/workflows/ci.yml`.
Tagged release packaging is driven by `.github/workflows/release.yml`.

## Runtime Dependency Notes

The application can launch without all external services, but feature availability depends on what is installed and reachable:

- `UPower`: expected for battery and power-source telemetry
- `asusd`: required for ASUS platform profile and battery-limit control; Aura/RGB remains disabled until an exact lighting interface contract is verified
- `supergfxd`: required for GPU mode control
- Writable sysfs access: required for some CPU and keyboard-backlight operations
- Tray support: depends on desktop support for StatusNotifierItem / AppIndicator integration

When these dependencies are missing or read-only, the UI is expected to degrade gracefully instead of crashing.

Current release behavior to expect:

- missing `asusd` -> profile and charge-limit controls stay visible but explain that `asusd` is required
- asusd without Aura/RGB -> RGB controls stay disabled while brightness-only sysfs support remains available when present
- missing `supergfxd` -> GPU mode controls stay visible but explain that `supergfxd` is required
- readable-but-not-writable CPU sysfs -> CPU telemetry still works, writes become read-only, and Diagnostics lists the blocked paths
- readable-but-not-writable keyboard backlight sysfs -> current brightness can still be shown while writes remain unavailable
- dynamic fan telemetry -> the UI adapts to the detected fan set instead of assuming a fixed one-fan or two-fan layout
- update checks are manual and UI-side; in-place replacement is attempted only for safe user-local direct-binary installs and otherwise falls back to the latest release page

## Documentation Index

Start here:

- [docs/QUICK_START.md](docs/QUICK_START.md)

User docs:

- [docs/BUILD.md](docs/BUILD.md)
- [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)
- [docs/PERMISSIONS.md](docs/PERMISSIONS.md)
- [docs/FEATURE_MATRIX.md](docs/FEATURE_MATRIX.md)
- [docs/UI_PAGES.md](docs/UI_PAGES.md)
- [docs/HARDWARE_SUPPORT.md](docs/HARDWARE_SUPPORT.md)
- [docs/HARDWARE_VALIDATION_TEMPLATE.md](docs/HARDWARE_VALIDATION_TEMPLATE.md)

Developer docs:

- [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)
- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/DBUS_API.md](docs/DBUS_API.md)
- [docs/DBUS_NOTES.md](docs/DBUS_NOTES.md)
- [docs/PROVIDER_MATRIX.md](docs/PROVIDER_MATRIX.md)
- [docs/GUI_SPEC.md](docs/GUI_SPEC.md)
- [docs/ROADMAP.md](docs/ROADMAP.md)
- [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md)
