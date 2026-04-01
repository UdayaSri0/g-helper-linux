v0.16
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

## Currently Implemented Features

Current source-backed features include:

- Telemetry dashboard with CPU temperature, GPU temperature, battery, power source, fans, and warnings
- CPU telemetry and generic Linux CPU controls
  - turbo boost
  - power mode
  - governor
  - energy performance preference
  - min/max frequency limits
  - per-core online/offline toggles
- GPU mode read/write through `supergfxd`
- ASUS performance profile read/write through `asusd`
- ASUS battery charge limit read/write through `asusd`
- Keyboard backlight brightness read/write through sysfs when permissions allow
- Battery, power, health, and time estimates from `UPower` with sysfs fallback for additional details
- RAM, swap, PSI, zram, zswap, and top memory process telemetry
- Diagnostics UI and diagnostics CLI
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
- Polished release packaging assets such as icons and release metadata cleanup

Some related domain types and traits already exist in `rog-core` and `rog-providers`, but they are not fully wired into runtime behavior yet.

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

## Build and Run Basics

Useful commands for local development:

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Important note: the repository CI expects `fmt`, `clippy`, and tests to pass. If local results differ, compare against `.github/workflows/ci.yml`.

## Runtime Dependency Notes

The application can launch without all external services, but feature availability depends on what is installed and reachable:

- `UPower`: expected for battery and power-source telemetry
- `asusd`: required for ASUS platform profile and battery-limit control
- `supergfxd`: required for GPU mode control
- Writable sysfs access: required for some CPU and keyboard-backlight operations
- Tray support: depends on desktop support for StatusNotifierItem / AppIndicator integration

When these dependencies are missing or read-only, the UI is expected to degrade gracefully instead of crashing.

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
- [docs/RELEASE_CHECKLIST.md](docs/RELEASE_CHECKLIST.md)
