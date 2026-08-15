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
- optional multi-metric `nvidia-smi` telemetry with deterministic GPU selection and a slower cached refresh cadence
- `asusd` integration for:
  - ASUS platform profile read/write
  - battery charge-limit read/write
- `supergfxd` integration for:
  - GPU mode read/write
  - reboot/logout hints
- keyboard backlight brightness via sysfs
- ASUS Aura/RGB through the exact asusd 6.3.8-6.4.0 contract and the allow-listed G615JMR target's
  single-target RGB protocol
- fan monitoring and safe fan controls for supported ASUS/Linux hardware
  - dynamic 0..N fan telemetry
  - writable hwmon manual percentage control when confirmed
  - optional RPM target when `fanN_target` is writable
  - sync mode and time-limited boost
  - Auto/BIOS restore path
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
- `GetConfiguration`
- `SetConfiguration`
- `ResetConfiguration`
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
- Memory
- Lighting
- Cooling
- Setup & Access
- Settings
- Diagnostics
- About

The tray currently supports:

- open main window
- open GPU page
- profile selection
- GPU mode selection
- About
- Quit
- close-to-tray window behavior with an explicit full-exit path

## In Progress or Still Evolving

These areas are real and useful today, but they are not yet fully mature.

### `asusd` coverage

Current status:

- profile and battery-limit support are implemented

Still evolving:

- richer capability probing
- broader Aura/RGB device and asusd-version coverage
- full alignment between daemon capability flags and underlying ASUS interfaces

### `supergfxd` coverage

Current status:

- current mode read/write works
- supported mode probing works
- UI choices use the backend-reported supported modes
- pending/logout/reboot/unsafe transition states are explicit
- missing service and external authorization failures remain distinct from root-helper access

Still evolving:

- richer backend-specific application-busy detail when future supergfxd APIs expose it

### CPU controls

Current status:

- generic Linux CPU telemetry and several control actions are implemented

Still evolving:

- permissions model for write access
- UX around partial or read-only control availability
- continued extraction of page constructors from the UI bootstrap; shell, theme, reusable widgets, and fan drawing are already separate modules

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

Partially implemented:

- core safety validation
- daemon DBus API surface
- capability-driven UI diagnostics

Still missing:

- verified asusd fan-curve backend
- generic hwmon curve writes, because curve point file formats are hardware-specific
- graphical curve editor beyond the current API/diagnostic surface

### Auto mode and policy automation

Planned but missing at runtime:

- daemon-side use of the `rog-core` policy model
- AC/Battery rule application
- manual override pause/resume flow
- UI for auto rules

Note:

- the policy model exists in `rog-core`, but it is not wired into `rog-helperd`

### Aura / RGB lighting

Implemented narrowly:

- exact `xyz.ljones.Asusd` 6.3.8-6.4.0 Aura ABI matching
- exact G615JMR target `0b05:19b6` native HID effect encoding and privileged transport
- backend priority/conflict suppression, duplicate/rate protection, root-only alias, capability
  gating, and sysfs brightness fallback

Still missing:

- physical validation on the G615JMR target (native HID has no reliable state readback)
- broader ASUS models, product IDs, descriptors, interfaces, and future asusd contracts
- ARGB, independently addressable zones, per-key RGB, and power states unless a future backend
  proves those capabilities

See `AURA_BACKEND_DISCOVERY.md` for the exact evidence and safety boundary.

### Persistent configuration

Implemented:

- versioned XDG `config.toml` with daemon-authoritative writes
- atomic replacement, safe defaults, field-level validation, future-field tolerance, and legacy
  `ui.toml` migration
- dedicated Settings page for lifecycle and dashboard preferences
- inert preferred charge limit, last manual profile, and fan-sync preference
- confirmed reset to defaults

Still missing:

- saved automation rules and daemon policy execution
- persistent verified fan curves
- automatic hardware application, intentionally deferred until an explicit safety model exists

### Typed daemon API payloads

Planned but missing:

- a more strongly typed contract between daemon and UI

Current state:

- the daemon uses string-keyed `a{sv}` payloads

## Priorities

The most useful next technical priorities, based on current implementation status, are:

1. Fan curves
2. Auto mode / policy integration
3. Broader Aura / RGB hardware validation
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
