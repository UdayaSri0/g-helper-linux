# Roadmap

This roadmap reflects the current repository state as implemented today. It is intentionally organized around what already exists, what is actively evolving, and what is still missing.

Older milestone-era roadmap text described several features as future work even after they had been implemented. This file is meant to correct that drift.

## Already Implemented

The following features are present in the current codebase.

### Core architecture

- Rust workspace with dedicated crates for:
  - shared domain model
  - providers
  - daemon
  - UI
  - CLI
- Session-DBus daemon for UI communication
- Capability-driven UI behavior
- Diagnostics CLI for service and DBus inspection

### Providers and integrations

- `UPower` integration for power source and battery telemetry
- `hwmon` integration for temperatures and fan RPMs
- `nvidia-smi` fallback for GPU temperature
- `asusd` integration for:
  - ASUS platform profile read/write
  - battery charge-limit read/write
- `supergfxd` integration for:
  - GPU mode read/write
  - reboot/logout hints
- keyboard backlight brightness via sysfs
- CPU telemetry and generic Linux CPU controls
- memory, swap, PSI, zram, and top-process telemetry

### Current daemon API surface

The daemon already exposes:

- `GetCaps`
- `GetState`
- `GetTelemetry`
- `GetCpuCaps`
- `GetCpuTelemetry`
- `GetCpuDiagnostics`
- `SetLighting`
- `SetProfile`
- `SetGpuMode`
- `SetBatteryLimit`
- CPU control setters

See [DBUS_API.md](DBUS_API.md) for the current details.

### Current UI surface

The UI currently ships these pages:

- Dashboard
- CPU
- GPU
- Battery
- RAM
- Lighting
- Diagnostics
- About

The tray currently supports:

- open main window
- open GPU page
- profile selection
- GPU mode selection
- About
- Quit

## In Progress or Still Evolving

These areas are real and useful today, but they are not yet fully mature.

### `asusd` coverage

Current status:

- profile and battery-limit support are implemented

Still evolving:

- richer capability probing
- Aura/RGB coverage
- full alignment between daemon capability flags and underlying ASUS interfaces

### `supergfxd` coverage

Current status:

- current mode read/write works
- supported mode probing works
- pending-action hinting exists

Still evolving:

- clearer UX around risky transitions
- better busy or unsafe switching detection

### CPU controls

Current status:

- generic Linux CPU telemetry and several control actions are implemented

Still evolving:

- permissions model for write access
- UX around partial or read-only control availability
- cleanup and maintainability of current single-file UI and daemon handling

### Diagnostics

Current status:

- diagnostics page exists
- CLI diagnostics exist
- daemon exposes capability and CPU diagnostics data

Still evolving:

- richer exported diagnostics
- clearer operator-facing docs
- better hardware coverage reporting

### Documentation and release polish

Current status:

- major docs now describe the real implementation
- the release flow now has shared package metadata, `.deb` packaging, a prefix-friendly tarball, desktop metadata, session DBus activation, and checksum generation

Still evolving:

- hardware support tracking
- broader install validation
- broader cross-distro AppImage runtime validation

## Planned or Missing

These are not implemented end-to-end in the current repository.

### Fan curves

Planned but missing:

- provider-backed fan curve read/write
- daemon fan-curve API
- fan-curve UI
- device-level fan-curve capability detection

Note:

- the domain model for fan curves and validation already exists in `rog-core`

### Auto mode and policy automation

Planned but missing at runtime:

- daemon-side use of the `rog-core` policy model
- AC/Battery rule application
- manual override pause/resume flow
- UI for auto rules

Note:

- the policy model exists in `rog-core`, but it is not wired into `rog-helperd`

### Aura / RGB lighting

Planned but missing:

- provider-backed Aura lighting support
- daemon support for RGB/effects
- UI support beyond current keyboard brightness and mode placeholders

### Persistent configuration

Planned but missing:

- persistent user settings
- saved automation rules
- durable control preferences

### Typed daemon API payloads

Planned but missing:

- a more strongly typed contract between daemon and UI

Current state:

- the daemon uses string-keyed `a{sv}` payloads

## Priorities

The most useful next technical priorities, based on current implementation status, are:

1. Fan curves
2. Auto mode / policy integration
3. Aura / RGB lighting
4. Stronger typed daemon/UI contract
5. Packaging and release readiness

## Suggested Labels

Type:

- `type:bug`
- `type:feature`
- `type:chore`
- `type:docs`

Area:

- `area:ui`
- `area:daemon`
- `area:providers`
- `area:core`
- `area:packaging`
- `area:ci`

Feature:

- `feature:telemetry`
- `feature:profiles`
- `feature:fans`
- `feature:gpuswitch`
- `feature:battery`
- `feature:lighting`
- `feature:automation`
- `feature:diagnostics`
- `feature:cpu`
- `feature:memory`

Priority:

- `prio:P0`
- `prio:P1`
- `prio:P2`

State:

- `status:blocked`
- `status:needs-hw-logs`
- `status:needs-design`

Community:

- `good first issue`
- `help wanted`
