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

## Missing `asusd`

Symptoms:

- performance profile controls are disabled
- battery charge-limit controls are disabled
- Dashboard or GPU page explains that `asusd` is required
- Diagnostics or `caps` reports `missing_backend` for profile and charge-limit access

Check:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- dbus --filter "asus|rog"
cargo run -p rog-cli -- caps
```

Expected current behavior:

- the UI keeps the profile and charge-limit rows visible
- the reason text explicitly mentions `asusd`
- Diagnostics includes the missing-backend state in the troubleshooting summary

What to do:

- install and start `asusd`
- restart `rog-helperd`
- recheck `caps` and the Dashboard

## Missing `supergfxd`

Symptoms:

- GPU mode controls are disabled
- the GPU page explains that `supergfxd` is required
- Diagnostics or `caps` reports `missing_backend` for GPU mode access

Check:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- dbus --filter "supergfx"
cargo run -p rog-cli -- caps
```

Expected current behavior:

- the GPU page keeps the GPU mode selector visible
- the reason text explicitly mentions `supergfxd`
- Diagnostics includes the missing-backend state in the troubleshooting summary

What to do:

- install and start `supergfxd`
- restart `rog-helperd`
- recheck the GPU page and Diagnostics

## Battery, Profile, or GPU Controls Still Unavailable

If `asusd` or `supergfxd` is present but the control is still unavailable, the current code distinguishes four common cases:

- `unsupported`
  - the backend is reachable, but the current machine does not expose that feature
- `missing_backend`
  - the required system service is not installed or not reachable
- `permission_denied`
  - the backend or sysfs path is visible but not writable by the current user
- `temporarily_unavailable`
  - the feature is expected to exist, but the current value could not be confirmed right now

Use:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- caps
```

Then compare the `*_access_status` and `*_access_reason` fields with what the Dashboard, GPU page, and Diagnostics show.

## CPU Telemetry Works, But CPU Controls Are Read-Only

This is the expected first-release behavior when the daemon can read CPU sysfs but cannot write the relevant control files.

Typical signs:

- CPU telemetry is visible
- the CPU page keeps the control rows visible
- affected controls are disabled
- the CPU banner says writes are blocked or backend support is missing
- Diagnostics reports `permission_denied` for one or more CPU control groups

Check:

```bash
cargo run -p rog-cli -- caps
busctl --user call io.github.roghelper.Daemon /io/github/roghelper/Daemon io.github.roghelper.Daemon1 GetCpuDiagnostics
ls -l /sys/devices/system/cpu/cpufreq/policy*/scaling_governor
ls -l /sys/devices/system/cpu/cpufreq/policy*/energy_performance_preference
ls -l /sys/devices/system/cpu/cpufreq/policy*/scaling_{min,max}_freq
ls -l /sys/devices/system/cpu/intel_pstate/no_turbo
ls -l /sys/devices/system/cpu/cpu*/online
```

Expected current behavior:

- telemetry remains visible
- only the affected controls are disabled
- Diagnostics lists the real sysfs paths and whether each one is readable or writable
- there is no fake `Fix permissions` action

What to do:

- inspect the exact blocked paths in Diagnostics
- grant `rog-helperd` write access only to the specific files you actually want writable
- restart the user daemon after changing local policy:

```bash
systemctl --user restart rog-helperd
```

Important note:

- the repository does not ship an automatic privilege escalation or permission-repair path for CPU sysfs writes

## CPU Counts Look Wrong On A Hybrid CPU

The current CPU UI distinguishes:

- physical core count
- logical thread count
- per-logical-CPU rows
- cpufreq policy ids

These are not the same number on many hybrid CPUs.

Common example:

- physical cores: `16`
- logical threads: `24`
- cpufreq policies: `24`

Expected current behavior:

- `cpu_count` is shown as physical cores
- `thread_count` is shown as logical threads
- the per-row table lists every logical CPU / thread in deterministic logical CPU order
- policy ids are shown as backend detail, not as the physical core count

If you suspect a mismatch, capture:

```bash
cargo run -p rog-cli -- caps
busctl --user call io.github.roghelper.Daemon /io/github/roghelper/Daemon io.github.roghelper.Daemon1 GetCpuDiagnostics
```

Then compare those values with the CPU page and Diagnostics text.

## Keyboard Backlight Is Visible, But Read-Only

The current runtime lighting backend is keyboard backlight brightness via sysfs.

Typical signs:

- the Dashboard or Lighting page shows the current brightness
- the brightness control is disabled or writes fail cleanly
- Diagnostics or `caps` shows that the feature is present but not writable

Check:

```bash
cargo run -p rog-cli -- caps
ls -l /sys/class/leds/*/brightness
```

Expected current behavior:

- current brightness can still be shown
- the control stays visible
- the UI does not pretend Aura / RGB support exists

What to do:

- if you want sysfs writes, create a local admin-managed permission change for the relevant LED `brightness` file
- restart `rog-helperd` after changing local policy

The repository does not currently ship a bundled udev rule or privileged helper for this path.

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

Fan telemetry specifics:

- fan detection is dynamic over the available `fan*_input` files and may report 0, 1, 2, 3, or more fan rows depending on the machine
- hwmon labels are used when available; otherwise the UI falls back to `Fan 1`, `Fan 2`, and so on
- a detected fan row can still show as unavailable if the platform exposes the input but not a usable current RPM value
- the Dashboard, details, and Diagnostics views all use the same detected fan set
- the Diagnostics page lists the detected hwmon device, raw `fan*_input` path, chosen display label, and current RPM state for each fan row

Useful fan checks:

```bash
ls -l /sys/class/hwmon/hwmon*/fan*_input
ls -l /sys/class/hwmon/hwmon*/fan*_label
cargo run -p rog-cli -- sensors
```

## Diagnostics Commands

These commands are useful for most current troubleshooting:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- dbus --filter "asus|rog|supergfx|power|upower"
cargo run -p rog-cli -- sensors
cargo run -p rog-cli -- caps
```

When you are doing real hardware validation instead of one-off troubleshooting, record the outputs in:

- [docs/HARDWARE_SUPPORT.md](HARDWARE_SUPPORT.md)
- [docs/HARDWARE_VALIDATION_TEMPLATE.md](HARDWARE_VALIDATION_TEMPLATE.md)

If documentation and runtime behavior still seem inconsistent, inspect the current source in:

- `crates/rog-daemon/src/main.rs`
- `crates/rog-ui/src/main.rs`
- `crates/rog-providers/src/`
