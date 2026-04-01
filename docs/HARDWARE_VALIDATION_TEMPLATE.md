# Hardware Validation Template

Use this template only after testing on a real machine.

Do not leave guessed values in a committed record.

Recommended status words:

- `Pass`
- `Fail`
- `Read-only`
- `Unsupported`
- `Temporarily unavailable`
- `Untested`
- `Not represented in current test fleet`

## Validation Record

### Metadata

- Tested date:
- Tester:
- Record path or identifier:

### Machine

- Model:
- Distro:
- Kernel:
- Desktop environment:
- CPU:
- GPU:
- BIOS / firmware:
- `asusd` version:
- `supergfxd` version:

### Startup / Basic Runtime

- Session daemon startup:
- UI startup:
- Tray visibility:
- Diagnostics CLI:
- Session DBus introspection:

### Core Controls

- Profile control:
- GPU mode control:
- Battery limit:
- Keyboard backlight:
- CPU telemetry:
- CPU write controls:

### CPU Topology / Telemetry

- Physical core count shown:
- Logical thread count shown:
- cpufreq policy count observed:
- Per-logical-CPU rows visible:
- Current CPU warnings:

### Fan Telemetry

- Fan telemetry count:
- Per-fan labels:
- Fan rows with RPM:
- Fan rows without RPM:

### Diagnostics / Capability States

- Missing `asusd` behavior observed:
- Missing `supergfxd` behavior observed:
- CPU readable-but-not-writable behavior observed:
- Keyboard backlight readable-but-not-writable behavior observed:
- Copy diagnostics action:

### Known Warnings

- Warning banner text:
- Diagnostics troubleshooting summary:
- Additional warnings:

### Unsupported / Unverified Items

- Unsupported on this machine:
- Untested during this session:
- Not represented in current test fleet:

### Evidence

- `cargo run -p rog-cli -- services`:
- `cargo run -p rog-cli -- dbus --filter "asus|rog|supergfx|power|upower"`:
- `cargo run -p rog-cli -- sensors`:
- `cargo run -p rog-cli -- caps`:
- `busctl --user introspect io.github.roghelper.Daemon /io/github/roghelper/Daemon`:
- CPU sysfs permission checks:
- LED brightness permission checks:
- Screenshots:

### Notes

- Additional observations:
- Recommended follow-up:

## Short Review Summary

- Release-relevant passes:
- Release-relevant failures:
- Items that remain unverified:
