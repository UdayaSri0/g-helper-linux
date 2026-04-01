# Troubleshooting

This document covers common issues based on the current implementation.

## `Cargo.lock` parse error (`version = 4`)

If you see:

```text
error: failed to parse lock file at: .../Cargo.lock
Caused by:
  lock file version 4 requires `-Znext-lockfile-bump`
```

your Cargo is too old for this workspace lockfile.

Fix by installing or updating Rust through `rustup`:

```bash
curl --proto '=https' --tlsv1.2 -sSf https://sh.rustup.rs | sh
source "$HOME/.cargo/env"
rustup toolchain install stable
rustup default stable
cargo --version
rustc --version
```

Then retry:

```bash
cargo build --workspace
```

## Session DBus Issues

The UI talks to `rog-helperd` over the session bus, not the system bus.

Verify that the daemon is visible on the user bus:

```bash
busctl --user list | rg -n "io\\.github\\.roghelper\\.Daemon"
busctl --user introspect io.github.roghelper.Daemon /io/github/roghelper/Daemon
busctl --user call io.github.roghelper.Daemon /io/github/roghelper/Daemon io.github.roghelper.Daemon1 GetTelemetry
```

If these commands fail, start the daemon manually and retry.

## Daemon Not Running

Start the daemon first:

```bash
cargo run -p rog-daemon
```

Then start the UI:

```bash
cargo run -p rog-ui
```

If you installed the user service, also check:

```bash
systemctl --user status rog-helperd --no-pager
```

Important note:

- the packaged user service expects `rog-helperd` to be available on `PATH`

## Tray Not Visible

The tray uses StatusNotifierItem through `ksni`.

Some desktop environments, especially GNOME, do not show tray icons by default unless an AppIndicator/SNI extension is enabled.

The UI is expected to continue running even if tray support is unavailable.

## `asusd` or `supergfxd` Unavailable

Profile, battery-limit, and GPU-mode features depend on external system services:

- `asusd`
  - ASUS platform profile
  - battery charge-limit control
- `supergfxd`
  - GPU mode control

Use the CLI to inspect the environment:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- dbus --filter "asus|rog|supergfx|power|upower"
```

If the relevant service is missing from the system bus or not running, those controls will be unavailable in the daemon and UI.

## Battery, Profile, or GPU Controls Unavailable

Possible causes:

- `asusd` is not installed or not reachable
- `supergfxd` is not installed or not reachable
- the backend is present but does not expose the expected capability
- the daemon is running, but the feature is not supported on the current machine

What to check:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- caps
```

If `caps` does not report the feature as available, the UI should disable or hide the corresponding controls.

## CPU Controls Read-Only

The current CPU controls use generic Linux sysfs interfaces.

This means the UI can show CPU telemetry even when writes are not allowed.

Read-only CPU controls typically mean:

- the current user cannot write the relevant sysfs files
- the daemon is running unprivileged, which is expected
- the current platform exposes telemetry but not writable control paths

What to check:

```bash
cargo run -p rog-cli -- caps
```

In the UI:

- the CPU page will still show telemetry
- controls may appear disabled or marked read-only

## Keyboard Backlight Read-Only

The current implemented lighting backend is keyboard backlight brightness via sysfs.

Common reasons it is read-only:

- `/sys/class/leds/.../brightness` is readable but not writable by the current user
- `asusd` is not available as an alternative backend

Current behavior:

- the UI can still report brightness
- the daemon can still expose the backend
- writes may fail or controls may become read-only

Recommended checks:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- dbus --filter "asus|rog"
```

If you want sysfs write access specifically, you may need a udev rule or another system-level permission change. The repository does not currently ship that policy for you.

## Missing Telemetry or Limited `hwmon` Coverage

Some systems expose only partial sensor coverage through `hwmon`.

Current implications:

- CPU or GPU temperatures may be missing
- individual fan RPMs may be missing
- NVIDIA dGPU temperature may need to come from `nvidia-smi`

Use:

```bash
cargo run -p rog-cli -- sensors
```

The current telemetry implementation is best-effort:

- `hwmon` is the primary source for temperatures and fans
- `nvidia-smi` is used as a fallback for GPU temperature
- battery and power state are supplemented by `UPower` and power-supply sysfs

If a metric is not exposed by your system, the UI will show it as unavailable rather than inventing a value.

## Diagnostics Commands

These commands are useful for most current troubleshooting:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- dbus --filter "asus|rog|supergfx|power|upower"
cargo run -p rog-cli -- sensors
cargo run -p rog-cli -- caps
```

If documentation and runtime behavior still seem inconsistent, inspect the current source in:

- `crates/rog-daemon/src/main.rs`
- `crates/rog-ui/src/main.rs`
- `crates/rog-providers/src/`
