# Troubleshooting

This document covers common issues based on the current implementation.

For one consolidated, read-only readiness report, open **Setup & Access** in the UI or run:

```bash
cargo run -p rog-cli -- setup-check
```

The report verifies live APIs where possible and lists binary, systemd, DBus, and sysfs evidence.
It does not elevate privileges or apply any repair automatically.

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

## Settings Do Not Persist or `config.toml` Is Invalid

The canonical configuration is:

```text
$XDG_CONFIG_HOME/rog-helper/config.toml
```

When `XDG_CONFIG_HOME` is unset, it is `$HOME/.config/rog-helper/config.toml`. The file is owned by
the current user; do not create it as root. `rog-helperd` is the authoritative writer, so confirm the
session daemon is reachable before changing Settings.

Useful checks:

```bash
systemctl --user status rog-helperd --no-pager
busctl --user call io.github.roghelper.Daemon /io/github/roghelper/Daemon io.github.roghelper.Daemon1 GetConfiguration
```

If TOML is malformed, the application continues with safe defaults and logs a warning; it does not
overwrite the broken file automatically. Correct the syntax or use the confirmed Reset to Defaults
action. A reset replaces only rog-helper's canonical configuration and updates the app-managed
autostart entry to match the default lifecycle state.

An older `$XDG_CONFIG_HOME/rog-helper/ui.toml` is imported only when `config.toml` does not yet
exist. The old file is deliberately retained for recovery. Saved charge-limit, profile, and fan-sync
values are not applied automatically at daemon startup or login.

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

- source-tree and portable-prefix service installs resolve `rog-helperd` from `PATH`
- `.deb` installs render an absolute `ExecStart=/usr/bin/rog-helperd`
- packaged desktop installs also ship session DBus activation metadata, so launching the UI can start the daemon even before the user service is enabled

## Tray Not Visible

The tray uses StatusNotifierItem through `ksni`.

Some desktop environments, especially GNOME, do not show tray icons by default unless an AppIndicator/SNI extension is enabled.

The UI is expected to continue running even if tray support is unavailable.

## Missing `asusd`

Symptoms:

- performance profile controls are disabled
- battery charge-limit controls are disabled
- Dashboard or GPU page explains that `asusd` is required
- Diagnostics or `caps` reports `missing_backend` for profile and charge-limit access when `asusd` is not installed or detected
- Diagnostics or `caps` reports `temporarily_unavailable` for profile and charge-limit access when `asusd` exists but the backend cannot be reached right now

Check:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- setup-check
cargo run -p rog-cli -- dbus --filter "asus|rog"
cargo run -p rog-cli -- caps
```

Expected current behavior:

- the UI keeps the profile and charge-limit rows visible
- the reason text explicitly mentions `asusd`
- Diagnostics includes either the missing-backend or temporarily-unavailable state in the troubleshooting summary

What to do:

- install and start `asusd`
- restart `rog-helperd`
- recheck `caps` and the Dashboard

## Missing `supergfxd`

Symptoms:

- GPU mode controls are disabled
- the GPU page explains that `supergfxd` is required
- Diagnostics or `caps` reports `missing_backend` for GPU mode access when `supergfxd` is not installed or detected
- Diagnostics or `caps` reports `temporarily_unavailable` for GPU mode access when `supergfxd` exists but the backend cannot be reached right now

Check:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- setup-check
cargo run -p rog-cli -- dbus --filter "supergfx"
cargo run -p rog-cli -- caps
```

Expected current behavior:

- the GPU page keeps the GPU mode selector visible
- the reason text explicitly mentions `supergfxd`
- Diagnostics includes either the missing-backend or temporarily-unavailable state in the troubleshooting summary

What to do:

- install and start `supergfxd`
- restart `rog-helperd`
- recheck the GPU page and Diagnostics

## Battery, Profile, or GPU Controls Still Unavailable

If `asusd` or `supergfxd` is present but the control is still unavailable, the current code distinguishes four common cases:

- `unsupported`
  - the backend is reachable, but the current machine does not expose that feature
- `missing_backend`
  - the required system service is not installed or was not detected on the system bus
- `permission_denied`
  - the backend or sysfs path is visible but not writable by the current user
- `temporarily_unavailable`
  - the feature is expected to exist, but the backend or current value could not be confirmed right now

Use:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- caps
```

Then compare the `*_access_status` and `*_access_reason` fields with what the Dashboard, GPU page, and Diagnostics show.

`cargo run -p rog-cli -- caps` now uses the same backend-failure split as the daemon for `asusd` and `supergfxd`:

- `missing_backend` means the service was not detected
- `temporarily_unavailable` means the service or DBus backend could not be reached right now

## CPU Telemetry Works, But CPU Controls Are Read-Only

This is expected behavior when the daemon can read CPU sysfs but cannot write the relevant control files.

## Fans Visible, But Controls Disabled

This is expected on many laptops. Linux often exposes `fan*_input` RPM telemetry without exposing safe writable control.

Check:

```bash
cargo run -p rog-cli -- fans
cargo run -p rog-cli -- fan-caps
find /sys/class/hwmon -maxdepth 3 -type f \( -name "fan*_input" -o -name "fan*_label" -o -name "pwm*" -o -name "fan*_target" \) -print
```

Expected behavior:

- fan rows remain visible
- the Fans page still shows animated RPM rotors and individual fan cards for telemetry
- manual controls are disabled unless `rog-helperd` reports writable `pwmN` plus `pwmN_enable`
- RPM target controls are disabled unless writable `fanN_target` exists
- Diagnostics lists endpoint paths and permission warnings

## Fan RPM Appears, But Manual Control Is Unavailable

`fan*_input` is normally read-only RPM telemetry. Manual percentage control needs a matching writable PWM endpoint.

Common reasons:

- the kernel driver exposes telemetry only
- `pwmN` or `pwmN_enable` does not exist
- the files exist but are not writable by the daemon user
- firmware/BIOS owns the fan policy and ignores software control

The app should degrade to read-only telemetry instead of trying unsafe writes.

## GPU Clock Shows `-- MHz`

The Fans page shows GPU core and memory clocks only when the daemon can read them safely. On NVIDIA systems this is best-effort `nvidia-smi` telemetry.

Common reasons clocks are unavailable:

- `nvidia-smi` is not installed
- the active GPU is AMD or Intel and no clock provider is exposed yet
- the NVIDIA GPU is powered down in hybrid/integrated mode
- the query timed out or the driver reported the clock as unsupported

The UI should show `-- MHz` and continue updating temperature and fan RPM telemetry.

## Permission Denied Writing PWM or Fan Target

If `fan-caps` reports permission warnings, inspect ownership:

```bash
ls -l /sys/class/hwmon/hwmon*/pwm* /sys/class/hwmon/hwmon*/fan*_target 2>/dev/null
```

Do not grant broad sysfs write access. If you choose to change local policy, grant only the specific confirmed fan-control files and restart `rog-helperd`.

## asusd Present, But Fan Curves Unsupported

Fan curves are hardware and asusd-interface dependent. If asusd does not expose a discoverable fan-curve API, `rog-helperd` reports curves unsupported rather than broken.

Useful inspection:

```bash
busctl --system list | grep -Ei "asus|rog|supergfx|power|upower"
cargo run -p rog-cli -- dbus --filter "asus|rog" --service xyz.ljones.Asusd --path / --depth 3
```

## Firmware or BIOS Overrides Curves

Some laptops accept a software fan value briefly and then firmware policy takes over. Use Auto/BIOS mode if fan behavior is inconsistent, and include `fans`, `fan-caps`, and hwmon path output in hardware validation notes.

## Fan Labels Unknown

If `fan*_label` is missing, the app uses `Fan 1`, `Fan 2`, and so on. It does not guess CPU/GPU mapping from position alone.

## Boost Failed or Restored Auto

Boost requires writable manual percentage support. It is always time-limited. If temperatures become unavailable or a critical temperature is observed during manual control, the daemon attempts to restore Auto/BIOS mode and records a warning.

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

The sysfs lighting fallback controls keyboard backlight brightness only.

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
- the UI does not enable RGB colour unless the daemon reports writable Aura/RGB support

What to do:

- if you want sysfs writes, create a local admin-managed permission change for the relevant LED `brightness` file
- restart `rog-helperd` after changing local policy

The repository does not currently ship a bundled udev rule or privileged helper for this path.

## Keyboard RGB / Aura Diagnostics

RGB colour requires an asusd Aura/keyboard lighting interface on system DBus.

Check:

```bash
gdbus introspect --system --dest xyz.ljones.Asusd --object-path /xyz/ljones --recurse
cargo run -p rog-cli -- lighting-diagnostics
cargo run -p rog-cli -- caps
cargo run -p rog-cli -- dbus --filter "asus|rog|aura|kbd|keyboard|led|rgb"
ls -la /sys/class/leds
cat /sys/class/leds/asus::kbd_backlight/brightness
cat /sys/class/leds/asus::kbd_backlight/max_brightness
```

Expected behavior:

- `has_aura: true` only when an Aura/RGB provider was actually detected
- `has_kbd_backlight: true` can still be true for brightness-only sysfs support
- the Lighting page enables the RGB picker only when `supports_rgb` is true
- if asusd is present but does not expose Aura/RGB, Diagnostics should say that RGB is not exposed by asusd
- if only sysfs is available, Diagnostics should explain that the active backend only supports brightness through `/sys/class/leds/...`
- if write access is blocked, Diagnostics should name the brightness file permission issue
- if a potential Aura-like interface is found but unsupported, include the introspection output in a GitHub issue

If RGB is unavailable, the app should keep brightness controls available when sysfs backlight support exists.

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
cargo run -p rog-cli -- dbus --filter "asus|rog|aura|kbd|keyboard|led|rgb|supergfx|power|upower"
cargo run -p rog-cli -- sensors
cargo run -p rog-cli -- caps
cargo run -p rog-cli -- lighting-diagnostics
```

When you are doing real hardware validation instead of one-off troubleshooting, record the outputs in:

- [docs/HARDWARE_SUPPORT.md](HARDWARE_SUPPORT.md)
- [docs/HARDWARE_VALIDATION_TEMPLATE.md](HARDWARE_VALIDATION_TEMPLATE.md)

If documentation and runtime behavior still seem inconsistent, inspect the current source in:

- `crates/rog-daemon/src/main.rs`
- `crates/rog-ui/src/main.rs`
- `crates/rog-providers/src/`
