# rog-helper

Linux-native control app for ASUS ROG laptops, built as a Rust workspace with a GTK/libadwaita UI, a user-session daemon, a provider layer for DBus/sysfs/procfs integration, and a diagnostics CLI.

This repository was reviewed and its markdown docs were refreshed against the current source code. When documentation and source disagree in the future, prefer the implementation in `crates/` and update the docs.

## Overview

`rog-helper` is designed to provide a single Linux-native control surface for ASUS laptop features that are otherwise split across multiple services and system interfaces.

The current codebase implements:

- A GTK4/libadwaita desktop application with tray support via `ksni`
- A session-DBus daemon (`rog-helperd`) that owns current state and control actions
- Provider modules for `asusd`, `supergfxd`, `UPower`, `hwmon`, CPU sysfs, keyboard backlight sysfs, battery sysfs, memory telemetry, and DBus diagnostics helpers
- A CLI (`rog-helper`) for diagnostics and environment inspection

The project is clearly beyond an initial scaffold, but it is still an early implementation. Several core features are working today, while other planned features are still missing or only partially modeled.

## Architecture Summary

The current runtime architecture is:

```text
UI / tray
  -> session DBus (`io.github.roghelper.Daemon`)
  -> rog-helperd
  -> providers
  -> system DBus (`asusd`, `supergfxd`, `UPower`) and sysfs/procfs / `nvidia-smi`
```

High-level responsibilities:

- `rog-ui`: unprivileged GTK/libadwaita UI and tray
- `rog-daemon`: user-session daemon, state owner, telemetry aggregator, and control router
- `rog-providers`: DBus/sysfs/procfs integration layer
- `rog-core`: shared domain model, validation, and policy types
- `rog-cli`: diagnostics and probing tool

See [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md) for the current runtime architecture.

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

Basic packaging assets are also included under `packaging/`:

- `packaging/systemd-user/rog-helperd.service`
- `packaging/desktop/rog-helper.desktop`
- `packaging/dbus-session/io.github.roghelper.Daemon.service`
- `packaging/metainfo/io.github.roghelper.UI.metainfo.xml`
- generated-on-demand hicolor PNG icon set under `packaging/desktop/icons/hicolor/`
- `packaging/scripts/build-deb.sh`
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
- Best-effort dynamic fan telemetry for 0..N fans, with friendly labels when hwmon exposes them
- Battery, power, health, and time estimates from `UPower` with sysfs fallback for additional details
- RAM, swap, PSI, zram, zswap, and top memory process telemetry
- Diagnostics UI and diagnostics CLI
- About page update/support section with manual GitHub release checks, release-note preview, source/support links, and issue-reporting action
- Official `rog-helper` logo wired into desktop packaging, launcher metadata, tray/window icon naming, and packaging scripts
- Session DBus API for the UI and other local clients

See:

- [docs/FEATURE_MATRIX.md](docs/FEATURE_MATRIX.md)
- [docs/DBUS_API.md](docs/DBUS_API.md)
- [docs/PROVIDER_MATRIX.md](docs/PROVIDER_MATRIX.md)
- [docs/UI_PAGES.md](docs/UI_PAGES.md)

## Current Missing or Incomplete Features

Important gaps in the current implementation:

- Fan curve editing and fan-curve daemon APIs
- Aura/RGB lighting support
- Live auto mode / policy automation integration
- Persistent user configuration
- Typed DBus payloads shared between daemon and UI
- Complete tested hardware support matrix
- Broader cross-distro install validation and fully self-contained AppImage runtime bundling

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
cargo run -p rog-cli -- dbus --filter "asus|rog|supergfx|power|upower"
cargo run -p rog-cli -- caps
```

## Release Installs

Tagged releases now ship Linux release assets instead of source-only tags:

- `.deb` packages for Debian/Ubuntu-style installs
- prefix-friendly Linux tarballs for `/usr/local`
- direct `rog-helper`, `rog-helperd`, and `rog-helper-ui` binaries for advanced user-local installs and the UI updater
- SHA256 checksum files for every published asset

See [docs/BUILD.md](docs/BUILD.md) for the current install paths and packaging commands.

## Debian / Ubuntu / Mint Installation

Direct `.deb` install:

```bash
sha256sum -c rog-helper-0.2.0-SHA256SUMS.txt --ignore-missing
sudo apt install ./rog-helper_0.2.0_amd64.deb
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

Remove cleanly with:

```bash
sudo apt remove rog-helper
systemctl --user daemon-reload
```

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
- `asusd`: required for ASUS platform profile and battery-limit control
- `supergfxd`: required for GPU mode control
- Writable sysfs access: required for some CPU and keyboard-backlight operations
- Tray support: depends on desktop support for StatusNotifierItem / AppIndicator integration

When these dependencies are missing or read-only, the UI is expected to degrade gracefully instead of crashing.

Current first-release behavior to expect:

- missing `asusd` -> profile and charge-limit controls stay visible but explain that `asusd` is required
- missing `supergfxd` -> GPU mode controls stay visible but explain that `supergfxd` is required
- readable-but-not-writable CPU sysfs -> CPU telemetry still works, writes become read-only, and Diagnostics lists the blocked paths
- readable-but-not-writable keyboard backlight sysfs -> current brightness can still be shown while writes remain unavailable
- dynamic fan telemetry -> the UI adapts to the detected fan set instead of assuming a fixed one-fan or two-fan layout
- update checks are manual and UI-side; in-place replacement is attempted only for safe user-local direct-binary installs and otherwise falls back to the latest release page

## Documentation Index

- [docs/ARCHITECTURE.md](docs/ARCHITECTURE.md)
- [docs/BUILD.md](docs/BUILD.md)
- [docs/ROADMAP.md](docs/ROADMAP.md)
- [docs/GUI_SPEC.md](docs/GUI_SPEC.md)
- [docs/TROUBLESHOOTING.md](docs/TROUBLESHOOTING.md)
- [docs/FEATURE_MATRIX.md](docs/FEATURE_MATRIX.md)
- [docs/DBUS_API.md](docs/DBUS_API.md)
- [docs/PROVIDER_MATRIX.md](docs/PROVIDER_MATRIX.md)
- [docs/DEVELOPMENT.md](docs/DEVELOPMENT.md)
- [docs/PERMISSIONS.md](docs/PERMISSIONS.md)
- [docs/UI_PAGES.md](docs/UI_PAGES.md)
- [docs/HARDWARE_SUPPORT.md](docs/HARDWARE_SUPPORT.md)
- [docs/HARDWARE_VALIDATION_TEMPLATE.md](docs/HARDWARE_VALIDATION_TEMPLATE.md)
- [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md)
