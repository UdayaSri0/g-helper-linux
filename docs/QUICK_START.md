# Quick Start

This page is the shortest path into the project, whether you want to run the app, inspect the source, or find the right documentation.

## Run From Source

Install build dependencies first. On Ubuntu/Debian:

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libgtk-4-dev libadwaita-1-dev
```

Build the workspace:

```bash
cargo build --workspace
```

Start the daemon:

```bash
cargo run -p rog-daemon
```

Start the UI in another terminal:

```bash
cargo run -p rog-ui
```

Run diagnostics when data or controls are missing:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- dbus --filter "asus|rog|supergfx|power|upower"
cargo run -p rog-cli -- caps
```

## Source File Map

Start with these files when tracing behavior:

- `crates/rog-core/src/model.rs`
  - shared telemetry, capability, and control data models
- `crates/rog-core/src/dbus_keys.rs`
  - shared DBus payload key names
- `crates/rog-core/src/policy.rs`
  - policy/control access types
- `crates/rog-providers/src/lib.rs`
  - provider module entry point
- `crates/rog-providers/src/asusd.rs`
  - ASUS platform profile and charge-limit provider
- `crates/rog-providers/src/supergfx.rs`
  - GPU mode provider
- `crates/rog-providers/src/cpu.rs`
  - CPU telemetry, sysfs controls, and diagnostics
- `crates/rog-providers/src/memory.rs`
  - RAM, swap, PSI, zram, zswap, and top-process telemetry
- `crates/rog-providers/src/upower.rs`
  - UPower battery and power telemetry
- `crates/rog-providers/src/power_supply.rs`
  - power-supply sysfs battery fallback/details
- `crates/rog-providers/src/hwmon.rs`
  - hwmon temperature and fan telemetry
- `crates/rog-providers/src/kbd_backlight.rs`
  - keyboard backlight sysfs support
- `crates/rog-daemon/src/main.rs`
  - session daemon, DBus service, telemetry aggregation, and action routing
- `crates/rog-ui/src/main.rs`
  - GTK/libadwaita UI, tray, page layout, DBus decoding, and action flows
- `crates/rog-cli/src/main.rs`
  - diagnostics CLI

Packaging and desktop integration entry points:

- `packaging/systemd-user/rog-helperd.service`
- `packaging/dbus-session/io.github.roghelper.Daemon.service`
- `packaging/desktop/rog-helper.desktop`
- `packaging/metainfo/io.github.roghelper.UI.metainfo.xml`
- `packaging/scripts/build-release-assets.sh`

## User Docs

- [BUILD.md](BUILD.md)
  - build, run, install, and release artifact usage
- [TROUBLESHOOTING.md](TROUBLESHOOTING.md)
  - common runtime issues and diagnostic commands
- [PERMISSIONS.md](PERMISSIONS.md)
  - permission model and why some controls may be read-only
- [FEATURE_MATRIX.md](FEATURE_MATRIX.md)
  - implemented, partial, and missing features
- [UI_PAGES.md](UI_PAGES.md)
  - current UI pages and what each page shows
- [HARDWARE_SUPPORT.md](HARDWARE_SUPPORT.md)
  - tested/validated hardware notes

## Developer Docs

- [DEVELOPMENT.md](DEVELOPMENT.md)
  - contributor workflow, crate ownership, and validation commands
- [ARCHITECTURE.md](ARCHITECTURE.md)
  - runtime architecture and component responsibilities
- [DBUS_API.md](DBUS_API.md)
  - session DBus service contract
- [DBUS_NOTES.md](DBUS_NOTES.md)
  - DBus implementation notes and conventions
- [PROVIDER_MATRIX.md](PROVIDER_MATRIX.md)
  - provider/backend responsibilities and feature ownership
- [GUI_SPEC.md](GUI_SPEC.md)
  - UI design/spec notes
- [ROADMAP.md](ROADMAP.md)
  - planned work and incomplete areas
- [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md)
  - release validation and packaging checklist

## Validation

Before sending a change, run the same broad checks used by CI:

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo test --workspace
```

When documentation and source disagree, prefer the current implementation in `crates/` and update the docs in the same change.
