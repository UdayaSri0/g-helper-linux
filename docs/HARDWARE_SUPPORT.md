# Hardware Support

Tested hardware coverage is still incomplete.

The current repository targets ASUS ROG laptops in general, but the codebase does not yet include a formal compatibility matrix or a large set of captured test results in the repo itself.

This document is intended to be the place where that information is collected over time.

## Current Status

- Supported hardware families: Unknown from current code/docs beyond the general ASUS ROG target
- Fully tested model list: not yet documented in the repository
- Known-good distro/kernel matrix: not yet documented in the repository
- Service-version compatibility matrix: not yet documented in the repository

## What the Current Code Assumes

The current implementation assumes some combination of:

- ASUS ROG laptop hardware
- Linux `hwmon` exposure for temperatures and fan RPMs
- `UPower` for battery and power telemetry
- `asusd` for ASUS profile and battery-limit control
- `supergfxd` for GPU mode control
- keyboard backlight exposure in `/sys/class/leds`

These assumptions are real in the current code, but they are not the same thing as verified hardware support.

## Important Warning

Do not treat presence of a provider module as proof that a given laptop model is fully supported.

Examples:

- a machine may expose battery telemetry but not writable charge-limit control
- a machine may expose fan RPMs but not fan curves
- a machine may expose keyboard brightness but not Aura / RGB controls
- a machine may have `supergfxd` installed but still require careful mode-switch handling

## Test Record Template

Use the template below when documenting a tested machine.

### Model Record

- Model:
- Distro:
- Kernel:
- Desktop environment:
- `asusd` version:
- `supergfxd` version:
- GPU configuration:
- BIOS / firmware notes:

### Working Features

- Session daemon startup:
- UI startup:
- Tray:
- Telemetry:
- Performance profile:
- Battery limit:
- GPU mode:
- Keyboard backlight:
- CPU controls:
- Diagnostics CLI:

### Broken or Missing Features

- Fan curves:
- Aura / RGB:
- Automation:
- Packaging / tray integration issues:
- Other notes:

## Suggested Table Format

| Model | Distro | Kernel | `asusd` | `supergfxd` | Working features | Broken features | Notes |
| --- | --- | --- | --- | --- | --- | --- | --- |
| _fill in_ | _fill in_ | _fill in_ | _fill in_ | _fill in_ | _fill in_ | _fill in_ | _fill in_ |

## What to Capture During Hardware Validation

When adding real hardware support data, capture:

- `cargo run -p rog-cli -- services`
- `cargo run -p rog-cli -- dbus --filter "asus|rog|supergfx|power|upower"`
- `cargo run -p rog-cli -- sensors`
- `cargo run -p rog-cli -- caps`
- session DBus verification against `rog-helperd`

If possible, also capture:

- whether keyboard backlight is writable
- whether CPU controls are writable
- whether `supergfxd` mode switching requires logout or reboot
- whether any daemon warnings are shown in the UI

## Unknown Areas That Still Need Real Testing

Unknown from current code/docs:

- exact model-by-model `asusd` compatibility
- exact model-by-model `supergfxd` compatibility
- which ASUS models expose usable fan-curve interfaces
- which models expose Aura / RGB interfaces that match the current domain model

Until those results are documented here, treat hardware support claims as provisional.
