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
  - Aura / RGB discovery is implemented separately; no Aura write contract is currently verified
  - fan curve support is modeled in the daemon/API, but this provider does not write ASUS fan curves until an asusd curve interface is discoverable and tested
  - if asusd is present without a fan-curve interface, fan curves are reported as unsupported rather than broken
  - interface compatibility across `asusd` versions is still a live concern

## `aura`

File: `crates/rog-providers/src/aura.rs`

- Purpose
  - read-only ASUS Aura/RGB DBus discovery and exact interface diagnostics
- Read / Write
  - Read-only discovery; no Aura writes are currently authorized
- Dependencies
  - system DBus
  - `asusd` service exposing an introspectable Aura/keyboard LED interface
- External services and paths
  - service candidates:
    - `xyz.ljones.Asusd`
    - `org.asuslinux.Daemon`
  - object paths and interface names are discovered through DBus introspection
- Current limitations
  - no hardcoded hardware model database
  - object paths are learned from the standard DBus root rather than guessed
  - Aura-looking method/property names do not enable controls by themselves
  - no target with an installed, introspectable Aura service was available to verify a control contract
  - if Aura is absent or unverified, the daemon keeps the sysfs brightness/Off/Static fallback
  - exact target evidence and the implementation gate are in `AURA_BACKEND_DISCOVERY.md`

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
  - Read
  - Optional write through `rog-helperd` only when matching control files are confirmed writable
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
  - `fan*_label`
  - `fan*_min`
  - `fan*_max`
  - `fan*_target`
  - `pwm*`
  - `pwm*_enable`
  - `pwm*_mode`
  - `pwm*_auto_point*`
  - optional label files
- Current limitations
  - fan mapping confidence is explicit: raw `fan*_label` values are hardware-labelled; generated display names remain unknown rather than authoritative mappings
  - CPU/GPU identification depends on hwmon names and labels
  - some hardware exposes only partial data
  - fan discovery is dynamic over `fan*_input` files and may yield zero, one, or many rows depending on the platform
  - raw `fan*_label` files are optional; when missing, the daemon/UI fall back to `Fan 1`, `Fan 2`, and so on
  - a detected fan input may still have no current RPM value, which is surfaced as unavailable rather than dropped from diagnostics
  - `pwmN`, `pwmN_enable`, and `fanN_target` are diagnostic candidates only; existence or writable file permissions do not establish safe hardware semantics
  - the ASUS WMI `asus_custom_fan_curve` hwmon device is probed read-only for complete `pwmN_auto_pointM_{temp,pwm}` pairs and enable values
  - a complete readable curve sets `fan_curve_readable`, but does not set `has_fan_curves`, `fan_curve_writable`, or any control capability
  - generic hwmon fan writes are withheld until a backend-specific implementation verifies supported ranges, fan/channel mapping, permissions, and a reliable firmware Auto restore path
  - permission-denied and candidate paths remain diagnostics, not fatal startup errors
  - current backend priority is: a future verified fan-control backend, read-only hwmon telemetry/curve discovery, then unsupported diagnostics

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
  - no RGB colour support
  - write access often depends on system-level permissions

## `lighting`

File: `crates/rog-providers/src/lighting.rs`

- Purpose
  - combine sysfs keyboard backlight and asusd Aura probe data into a copyable lighting diagnostics report
- Read / Write
  - Read only diagnostics helper
- Dependencies
  - `kbd_backlight`
  - `aura`
  - `rog-core` lighting diagnostics model
- Current limitations
  - it reports backend capability and probe evidence; it does not perform hardware writes itself
  - report quality depends on what DBus introspection and sysfs permissions expose on the host

## `nvidia_smi`

File: `crates/rog-providers/src/nvidia_smi.rs`

- Purpose
  - optional read-only NVIDIA temperature, utilisation, VRAM, clock, power, and identity telemetry
- Read / Write
  - Read only
- Dependencies
  - `nvidia-smi` binary
- External services and paths
  - one multi-field `nvidia-smi --query-gpu=... --format=csv,noheader,nounits` invocation per refresh
- Current limitations
  - hwmon remains authoritative for the shared GPU temperature field
  - the daemon refreshes this provider every three seconds and caches values between ticks
  - multiple GPUs are parsed and the lowest NVIDIA index is selected deterministically
  - individual `N/A` or malformed metrics remain absent
  - depends on `PATH`
  - command absence, driver unload, failure, or timeout is handled as optional unavailability

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

## `setup`

File: `crates/rog-providers/src/setup.rs`

- Purpose
  - aggregate bounded, unprivileged readiness checks for Setup & Access and the CLI
- Read / Write
  - Read only
- Dependencies
  - existing asusd, supergfxd, UPower, NVIDIA, CPU, keyboard-lighting, and fan probes
  - session/system DBus, `systemctl show`, PATH lookup, and sysfs metadata
- Current behavior
  - live expected API calls determine readiness
  - executable and systemd-unit discovery is retained only as diagnostic evidence
  - NVIDIA relevance is based on detected PCI display hardware
  - no command is executed for remediation and no permission is changed
- Current limitations
  - service checks use bounded timeouts and report unreachable when an API stalls
  - package installation guidance remains documentation-based because package names and commands vary by distribution

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

- profile, battery limit, GPU mode, sysfs keyboard brightness, CPU control, and multiple telemetry sources are implemented
- Aura/RGB has read-only DBus discovery and capability diagnostics, but no verified write adapter yet
- fan curves and a unified telemetry abstraction are not implemented end-to-end

When updating provider-related docs, compare against:

- `crates/rog-providers/src/lib.rs`
- `crates/rog-providers/src/traits.rs`
- `crates/rog-daemon/src/main.rs`
