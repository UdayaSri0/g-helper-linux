# Architecture

This document describes the current runtime architecture as implemented in the repository today. It is based on the code in `crates/rog-core`, `crates/rog-providers`, `crates/rog-daemon`, `crates/rog-ui`, and `crates/rog-cli`.

## Layered Structure

The project is split into three main runtime layers, plus a shared model layer and a diagnostics CLI:

1. UI (`rog-helper-ui`)
   - Unprivileged GTK4/libadwaita desktop application
   - Tray support via `ksni`
   - Talks only to the session daemon
2. Daemon (`rog-helperd`)
   - Unprivileged user-session service
   - Owns current application state
   - Aggregates telemetry
   - Routes control actions to providers
   - Exposes a session DBus API
3. Providers (`rog-providers`)
   - System DBus clients
   - Sysfs/procfs readers and writers
   - External command fallback for NVIDIA temperature

Shared model layer:

- `rog-core`
  - Domain types
  - Validation helpers
  - Policy model
  - Error types

Diagnostics CLI:

- `rog-helper`
  - Direct provider and environment inspection
  - Useful when the daemon or UI is unavailable

## Runtime Data Flow

The current runtime flow is:

```text
GTK UI / tray
  -> session DBus (`io.github.roghelper.Daemon`)
  -> rog-helperd
  -> provider modules
  -> system DBus (`asusd`, `supergfxd`, `UPower`)
     and sysfs/procfs / `nvidia-smi`
```

More concretely:

- The UI fetches consolidated state from the daemon over the session bus.
- The daemon polls providers for telemetry and current control state.
- The daemon applies user actions by calling provider write methods.
- Providers talk to either:
  - system services over DBus
  - kernel/system interfaces in sysfs or procfs
  - `nvidia-smi` as a read-only fallback

Hardware I/O is not implemented in the UI.

## State Ownership

The daemon is the primary state owner.

Current state in `rog-helperd` includes:

- `AppState`
  - device capabilities
  - telemetry snapshot
  - CPU capability summary
  - CPU telemetry
  - warning list
- `ControlState`
  - current ASUS performance profile
  - current GPU mode
  - current battery charge limit

These are held in-memory behind `RwLock`s in `crates/rog-daemon/src/main.rs`.

The UI keeps a cached mirror of daemon state in `SharedUiState` inside `crates/rog-ui/src/main.rs`. It is not authoritative and is refreshed continuously from the daemon.

There is no persistent configuration file in current code. The workspace includes `toml` as a dependency at the workspace level, but runtime configuration persistence is not currently implemented.

## Polling Model

The current architecture is polling-based.

### Daemon polling

`rog-helperd` runs a 1 Hz telemetry loop. Each tick:

- reads `hwmon` telemetry
- reads memory and swap telemetry
- updates vmstat-based swap rates
- refreshes top memory users on a slower cadence
- supplements battery details from power-supply sysfs
- uses `nvidia-smi` as a fallback GPU temperature source
- reads `UPower` battery and power-source state
- refreshes CPU telemetry
- refreshes current profile, GPU mode, and battery limit when their backends are available

### UI polling

The UI uses two refresh loops:

- a background Tokio runtime that fetches daemon state once per second
- a GTK timeout that redraws widgets every 250 ms from cached shared state

This keeps UI code simple, but it also means the current design is not event-driven. There is no signal subscription model for telemetry updates or control-state changes.

## Capability Model

The current UI and daemon are capability-driven.

The main capability structure is `DeviceCaps` in `crates/rog-core/src/model.rs`. It includes flags such as:

- `has_profiles`
- `has_fan_curves`
- `has_fan_reading`
- `has_charge_limit`
- `has_gpu_modes`
- `has_aura`
- `has_kbd_backlight`
- `requires_reboot_for_gpu_switch`

It also carries:

- `endpoints`
- `notes`

These are used by the daemon and surfaced in the UI diagnostics view.

Important implementation note:

- `has_profiles`, `has_charge_limit`, `has_gpu_modes`, `has_fan_reading`, and `has_kbd_backlight` are actively populated today.
- `has_aura` and `has_fan_curves` exist in the model, but the current daemon does not probe them to true.

That means the capability model is broader than the current provider coverage.

## Provider Scope

The current provider layer is wider than the original early architecture docs suggested.

Implemented provider modules include:

- `asusd`
- `supergfx`
- `upower`
- `hwmon`
- `cpu`
- `kbd_backlight`
- `power_supply`
- `memory`
- `nvidia_smi`
- `dbus`

These modules cover:

- system DBus integration
- CPU sysfs reads and writes
- keyboard backlight sysfs reads and writes
- battery sysfs reads
- memory telemetry from procfs and sysfs
- DBus diagnostics helpers

See [PROVIDER_MATRIX.md](PROVIDER_MATRIX.md) for module-by-module details.

## Safety and Permission Model

The UI is intentionally unprivileged.

Current safety boundary:

- The UI does not directly touch system DBus, sysfs, or procfs for control operations.
- The daemon mediates control actions.
- The UI enables, disables, or hides controls based on reported capabilities.

Current permission reality:

- The daemon is also unprivileged.
- Some system DBus services may reject writes.
- Some sysfs files may be readable but not writable by the user.
- CPU controls and keyboard backlight writes may therefore degrade to read-only behavior.

There is no privileged helper and no polkit workflow in the current repository.

## DBus Contract Shape

The current daemon API uses string-keyed `a{sv}` payloads for most responses rather than strongly typed shared DBus structs.

Benefits:

- Flexible compatibility surface
- Easy to extend with optional keys

Tradeoffs:

- Tight string-key coupling between daemon and UI
- Manual serialization in `rog-daemon`
- Manual decoding in `rog-ui`
- More room for drift if fields are renamed without updating both sides

See [DBUS_API.md](DBUS_API.md) for the current wire-level API.

## Current Architectural Limitations

The current architecture has several known limitations:

- Policy automation exists as a model in `rog-core`, but it is not wired into runtime daemon behavior
- Fan curve support is modeled but not implemented end-to-end
- Aura/RGB lighting support is modeled but not implemented
- The daemon and UI each live mostly in a single large source file
- The UI and daemon duplicate some formatting and payload-shape logic
- Polling is simple but not especially efficient compared with a signal-driven model
- Typed shared DBus payloads are not yet in place

These limitations are important when reading older roadmap and GUI documents. Some design ideas are already present in the model layer, while the runtime still exposes only a subset of that design.
