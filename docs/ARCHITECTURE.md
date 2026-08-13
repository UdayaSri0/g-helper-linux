# Architecture

This document describes the current runtime architecture as implemented in the repository today. It is based on the code in `crates/rog-core`, `crates/rog-providers`, `crates/rog-daemon`, `crates/rog-privileged`, `crates/rog-ui`, and `crates/rog-cli`.

## Layered Structure

The project is split into four main runtime layers, plus a shared model layer and a diagnostics CLI:

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
   - Optional external-command NVIDIA telemetry
4. Privileged helper (`rog-helper-privileged`)
   - Minimal root system service, activated on demand
   - Owns `io.github.roghelper.Privileged` on the system bus
   - Uses PolicyKit actions tied to the calling system-bus peer
   - Exposes typed, validated CPU writes plus discovery and authorization probes

Shared model layer:

- `rog-core`
  - Domain types
  - Versioned configuration schema and atomic XDG persistence helpers
  - Validation helpers
  - Policy model
  - Error types

Diagnostics CLI:

- `rog-helper`
  - Direct provider and environment inspection
  - Useful when the daemon or UI is unavailable
  - `setup-check` reports the same service-readiness and permission concepts from a terminal

## Runtime Data Flow

The current runtime flow is:

```text
GTK UI / tray
  -> session DBus (`io.github.roghelper.Daemon`)
  -> rog-helperd
     -> provider modules
        -> system DBus (`asusd`, `supergfxd`, `UPower`)
           and sysfs/procfs / `nvidia-smi`
     -> system DBus (`io.github.roghelper.Privileged`)
        -> rog-helper-privileged (PolicyKit-gated, approved operations only)
```

More concretely:

- The UI fetches consolidated state from the daemon over the session bus.
- The daemon polls providers for telemetry and current control state.
- The daemon applies user actions by calling provider write methods.
- Providers talk to either:
  - system services over DBus
  - kernel/system interfaces in sysfs or procfs
  - `nvidia-smi` as an optional read-only NVIDIA telemetry source

Hardware I/O is not implemented in the UI.

Setup readiness follows the same boundary. Structured `SetupStatus`, `DependencyStatus`,
`PermissionStatus`, and `SetupIssue` types live in `rog-core`; live discovery is implemented in
`rog-providers`; `rog-helperd` exposes the result over session DBus; and the UI only presents it.
API calls are the readiness authority, while binary and systemd discovery is retained as evidence.

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

Persistent configuration is a versioned `AppConfig` model in `rog-core`. The canonical path is
`$XDG_CONFIG_HOME/rog-helper/config.toml`, falling back to
`$HOME/.config/rog-helper/config.toml`. The daemon loads it, owns the in-memory authoritative copy,
and is the sole writer through `GetConfiguration`, `SetConfiguration`, and `ResetConfiguration`.
The UI may read the same file at process startup so start-minimized behavior is available before
DBus connects, but all subsequent persistence goes through the daemon.

Configuration ownership is divided by section:

- `ui`: UI/lifecycle behavior such as close, login startup, and tray hints
- `dashboard`: optional panels and compact layout
- `controls`: remembered charge-limit/profile/fan-sync preferences

Version 1 serializes as:

```toml
version = 1

[ui]
close_behavior = "minimize_to_tray"
launch_on_login = false
start_minimized_to_tray = false
close_to_tray_hint_shown = false
fan_warning_acknowledged = false

[dashboard]
show_system_health = true
show_nvme = true
show_cooling_snapshot = true
compact = false

[controls]
# preferred_charge_limit = 80   # optional, validated to 40..=100
# last_manual_profile = "Turbo" # optional, remembered only
fan_sync_enabled = false
```

Control preferences are inert metadata. Loading configuration never applies a hardware action.
The previous `rog-helper/ui.toml` is migrated only when `config.toml` is absent; the legacy file is
left untouched. Writes use a temporary file in the destination directory, `sync_all`, and atomic
rename, so a failed replacement does not destroy the last good file. Malformed files fall back to
defaults without blocking daemon or UI startup, while valid fields survive individual invalid
values and unknown/future fields are tolerated during reads.

## Polling Model

The current architecture is polling-based.

### Daemon polling

`rog-helperd` runs a 1 Hz telemetry loop. Each tick:

- reads `hwmon` telemetry
- reads memory and swap telemetry
- updates vmstat-based swap rates
- refreshes top memory users on a slower cadence
- supplements battery details from power-supply sysfs
- refreshes optional NVIDIA telemetry every three seconds with one multi-field `nvidia-smi` query, caching it between daemon ticks; hwmon remains the preferred GPU temperature source
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

- `has_profiles`, `has_charge_limit`, `has_gpu_modes`, `has_fan_reading`, `has_kbd_backlight`, and `has_aura` are actively populated today.
- `has_fan_curves` exists in the model, but the current daemon does not probe it to true.

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

The privileged boundary handles only supported CPU controls that fail the normal provider route with
a write-permission error. Telemetry and directly writable controls stay unprivileged. The helper
discovers fixed CPU sysfs endpoints itself and never accepts caller-provided paths.
`rog-helperd` probes it only when diagnostics are requested and continues normally when it is
missing, blocked, incompatible, or unavailable. The helper cannot receive filesystem paths,
program names, shell text, or arbitrary values to write. Future control methods must map a
validated domain operation to a fixed internal endpoint and authorize the original system-bus
caller with the matching application-specific PolicyKit action.

## DBus Contract Shape

The current daemon API uses string-keyed `a{sv}` payloads for most responses rather than strongly typed shared DBus structs.

Benefits:

- Flexible compatibility surface
- Easy to extend with optional keys

Tradeoffs:

- External clients still depend on documented string keys
- Manual serialization in `rog-daemon`
- Manual domain decoding in `rog-ui`, using shared lossless map helpers
- Leaf-row fields not yet moved into shared constants still require coordinated changes

The highest-risk keys and status semantics are shared through `rog-core`, without changing the
wire format. See [DBUS_API.md](DBUS_API.md) for the public API and
[DBUS_CONTRACT_MAP.md](DBUS_CONTRACT_MAP.md) for the encoder/decoder/default audit.

## Current Architectural Limitations

The current architecture has several known limitations:

- Policy automation exists as a model in `rog-core`, but it is not wired into runtime daemon behavior
- Fan curve support is modeled but not implemented end-to-end
- Aura/RGB lighting depends on asusd exposing a compatible introspectable backend and still needs broad hardware validation
- The daemon remains mostly in one large source file; UI state/update wiring remains in `main.rs`, while the shell, theme, reusable widgets, fan drawing, and generic DBus decoding are separate modules
- The UI and daemon still duplicate some presentation formatting and lower-risk leaf-row shape logic
- Polling is simple but not especially efficient compared with a signal-driven model
- Typed shared DBus payloads are not yet in place
- Saved control preferences are deliberately not an automation engine and are never auto-applied

These limitations are important when reading older roadmap and GUI documents. Some design ideas are already present in the model layer, while the runtime still exposes only a subset of that design.
