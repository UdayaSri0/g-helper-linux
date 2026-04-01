# Provider Matrix

This document summarizes the current provider modules in `crates/rog-providers/src/`.

For each module, it describes:

- purpose
- whether it reads or writes state
- dependencies
- external service names or file paths
- current risks or limitations

## `asusd`

File: `crates/rog-providers/src/asusd.rs`

- Purpose
  - ASUS platform profile and battery charge-limit integration
- Read / Write
  - Read + Write
- Dependencies
  - system DBus
  - `asusd`-compatible platform interface
- External services and paths
  - current hardcoded endpoint candidates:
    - `xyz.ljones.Asusd` / `/xyz/ljones` / `xyz.ljones.Platform`
    - `org.asuslinux.Daemon` / `/org/asuslinux` / `org.asuslinux.Platform`
- Current limitations
  - only profile and charge-limit coverage are implemented
  - Aura / RGB and fan curves are not implemented here yet
  - interface compatibility across `asusd` versions is still a live concern

## `supergfx`

File: `crates/rog-providers/src/supergfx.rs`

- Purpose
  - GPU mode discovery and switching
- Read / Write
  - Read + Write
- Dependencies
  - system DBus
  - `supergfxd`
- External services and paths
  - `org.supergfxctl.Daemon`
  - `/org/supergfxctl/Gfx`
  - `org.supergfxctl.Daemon`
- Current limitations
  - current safety model is based on supported modes and pending-action hints
  - there is no richer runtime “dGPU busy” heuristic yet
  - some backend-specific modes are collapsed into a simpler app model

## `upower`

File: `crates/rog-providers/src/upower.rs`

- Purpose
  - battery and power-source telemetry from standard Linux power services
- Read / Write
  - Read only
- Dependencies
  - system DBus
  - `UPower`
- External services and paths
  - `org.freedesktop.UPower`
  - `/org/freedesktop/UPower`
  - `org.freedesktop.UPower.Device` objects
- Current limitations
  - battery and line-power state are best-effort
  - telemetry depends on what the host `UPower` implementation exposes

## `hwmon`

File: `crates/rog-providers/src/hwmon.rs`

- Purpose
  - temperature and fan-RPM telemetry from Linux hwmon sysfs
- Read / Write
  - Read only
- Dependencies
  - `/sys/class/hwmon`
- External services and paths
  - `/sys/class/hwmon`
  - `temp*_input`
  - `fan*_input`
  - optional label files
- Current limitations
  - sensor mapping is heuristic
  - CPU/GPU identification depends on hwmon names and labels
  - some hardware exposes only partial data
  - fan discovery is dynamic over `fan*_input` files and may yield zero, one, or many rows depending on the platform
  - raw `fan*_label` files are optional; when missing, the daemon/UI fall back to `Fan 1`, `Fan 2`, and so on
  - a detected fan input may still have no current RPM value, which is surfaced as unavailable rather than dropped from diagnostics

## `cpu`

File: `crates/rog-providers/src/cpu.rs`

- Purpose
  - generic Linux CPU telemetry and control
- Read / Write
  - Read + Write
- Dependencies
  - sysfs
  - procfs
  - Intel RAPL when available
- External services and paths
  - `/sys/devices/system/cpu`
  - `/sys/devices/system/cpu/cpufreq`
  - `/proc/stat`
  - `/sys/class/powercap`
  - Intel `no_turbo` and generic `cpufreq/boost` control files
- Current limitations
  - generic Linux backend, not ASUS-specific
  - write support depends on sysfs permissions
  - probes and reports per-control readable/writable sysfs paths instead of exposing only a coarse write boolean
  - the repository still does not bundle a privileged helper, polkit flow, or udev rule for CPU writes
  - power-mode application may intentionally use only governor or only EPP when that is all the platform exposes
  - current code still needs maintenance cleanup

## `kbd_backlight`

File: `crates/rog-providers/src/kbd_backlight.rs`

- Purpose
  - keyboard backlight brightness probing and sysfs write support
- Read / Write
  - Read + Write
- Dependencies
  - LED sysfs entries
- External services and paths
  - `/sys/class/leds`
  - preferred LED name: `asus::kbd_backlight`
  - `brightness`
  - `max_brightness`
- Current limitations
  - brightness only
  - no Aura / RGB support
  - write access often depends on system-level permissions

## `nvidia_smi`

File: `crates/rog-providers/src/nvidia_smi.rs`

- Purpose
  - read-only fallback GPU temperature source
- Read / Write
  - Read only
- Dependencies
  - `nvidia-smi` binary
- External services and paths
  - external command: `nvidia-smi --query-gpu=temperature.gpu --format=csv,noheader,nounits`
- Current limitations
  - only a fallback path
  - depends on `PATH`
  - command absence or timeout is expected to be handled gracefully

## `power_supply`

File: `crates/rog-providers/src/power_supply.rs`

- Purpose
  - supplement battery telemetry from Linux power-supply sysfs
- Read / Write
  - Read only
- Dependencies
  - `/sys/class/power_supply`
- External services and paths
  - battery directories under `/sys/class/power_supply`
  - fields such as `status`, `cycle_count`, `energy_full`, `energy_full_design`, `power_now`, `current_now`, and `voltage_now`
- Current limitations
  - chooses a best-effort battery directory
  - depends on platform field naming and presence

## `memory`

File: `crates/rog-providers/src/memory.rs`

- Purpose
  - RAM, swap, PSI, zram, zswap, and top-process telemetry
- Read / Write
  - Read only
- Dependencies
  - procfs
  - selected sysfs files
- External services and paths
  - `/proc/meminfo`
  - `/proc/vmstat`
  - `/proc/pressure/memory`
  - `/proc/swaps`
  - `/proc/<pid>/status`
  - `/etc/passwd`
  - `/sys/module/zswap/parameters/enabled`
- Current limitations
  - best-effort process scanning
  - some fields depend on kernel support
  - top-process output mixes telemetry with presentation-oriented formatting

## `dbus`

File: `crates/rog-providers/src/dbus.rs`

- Purpose
  - generic system-bus discovery and introspection helpers
- Read / Write
  - Read only
- Dependencies
  - system DBus
- External services and paths
  - arbitrary system DBus names and object paths
  - `org.freedesktop.DBus.Introspectable`
- Current limitations
  - intended for diagnostics and probing
  - current introspection parsing is lightweight and regex-based

## `traits`

File: `crates/rog-providers/src/traits.rs`

- Purpose
  - define the intended provider abstraction surface
- Read / Write
  - Not a runtime backend
- Dependencies
  - `rog-core` domain types
- External services and paths
  - none directly
- Current limitations
  - trait surface is broader than current backend coverage
  - `FanProvider`, `LightingProvider`, and `TelemetryProvider` are not matched by full current runtime implementations

## Summary

The provider layer already covers a meaningful amount of real system integration, but it is still uneven:

- profile, battery limit, GPU mode, CPU control, and multiple telemetry sources are implemented
- Aura / RGB, fan curves, and a unified telemetry abstraction are not implemented end-to-end

When updating provider-related docs, compare against:

- `crates/rog-providers/src/lib.rs`
- `crates/rog-providers/src/traits.rs`
- `crates/rog-daemon/src/main.rs`
