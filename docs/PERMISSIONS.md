# Permissions

This document explains the current permission model used by the repository.

## Why the UI Is Unprivileged

The GTK UI is intentionally unprivileged.

Current goals of that design:

- keep hardware I/O out of the UI
- avoid requiring the desktop application itself to run as root
- centralize control logic in the daemon
- make unsupported or permission-limited features degrade cleanly

This matches the current implementation in `crates/rog-ui/src/main.rs` and `crates/rog-daemon/src/main.rs`.

## What the Daemon Does

`rog-helperd` is also unprivileged in the current repository.

It runs as a user-session service and is responsible for:

- capability probing
- telemetry aggregation
- DBus API export on the session bus
- routing write actions to provider backends

Important current note:

- the daemon is not a privileged helper
- it does not bypass kernel, sysfs, or system-DBus permission rules

## Current Permission Boundaries

The actual permission model depends on the backend in use.

### Session bus

- The UI talks to the daemon over the session bus.
- This is expected to work for the current logged-in user when the daemon is running.

### System bus

- Some provider backends depend on system DBus services such as `asusd`, `supergfxd`, and `UPower`.
- Those services may or may not allow the current user or daemon to perform writes.

### Sysfs and procfs

- Telemetry reads typically work with normal user access.
- Writes depend on the ownership and mode of the relevant sysfs file.

## What Currently Depends on System Permissions

### ASUS profile and battery-limit control

Backend:

- `crates/rog-providers/src/asusd.rs`

Depends on:

- system DBus
- a reachable and compatible `asusd` platform interface
- service-side permission policy

Current effect of failure:

- the daemon marks the feature unavailable
- the UI disables or hides the relevant controls

### GPU mode control

Backend:

- `crates/rog-providers/src/supergfx.rs`

Depends on:

- system DBus
- a reachable and compatible `supergfxd`

Current effect of failure:

- GPU mode controls are unavailable in the UI
- diagnostics should still show the missing capability

### CPU controls

Backend:

- `crates/rog-providers/src/cpu.rs`

Depends on:

- writable CPU sysfs files

Examples:

- scaling governor
- EPP
- boost toggle
- min/max frequency limits
- per-core online state

Current effect of failure:

- CPU telemetry can still work
- `policy_writable` becomes false
- the UI shows controls as read-only

### Keyboard backlight brightness

Backend:

- `crates/rog-providers/src/kbd_backlight.rs`

Depends on:

- writable LED brightness sysfs entry

Current effect of failure:

- brightness can still be readable
- current backend remains visible
- UI shows the control as read-only or reports a write failure

## Sysfs Write Limitations

The repository does not currently ship any special privilege escalation or policy configuration for sysfs writes.

That means:

- keyboard brightness may be readable but not writable
- CPU controls may be visible but not writable
- behavior depends on the current distro, kernel, udev rules, and file ownership

This is expected behavior in the current design.

## System DBus Expectations

The repository assumes these service categories may exist:

- `UPower`
- `asusd`
- `supergfxd`

The current code handles absence by degrading capabilities rather than treating them as fatal startup errors.

However:

- the daemon will still be less useful without them
- profile, GPU, and battery-limit control all depend on backend availability

## Read-Only Degradation Behavior

The current implementation prefers graceful degradation over hidden failure.

Current examples:

- feature capability is false -> UI disables or hides the control
- telemetry available but writes not allowed -> UI shows read-only state
- backend missing -> warnings and diagnostics expose the condition
- session daemon missing -> UI surfaces daemon connectivity problems

This is the intended current behavior and should be preserved when adding new backends.

## What the Repository Does Not Currently Provide

The repository does not currently include:

- a privileged helper binary
- bundled polkit rules
- bundled udev rules
- a root daemon
- a custom permission broker

If such a system is added later, this document should be updated to reflect the new trust and permission boundaries.

## Practical Guidance

When a feature is visible but not writable, inspect:

- `docs/TROUBLESHOOTING.md`
- `crates/rog-providers/src/cpu.rs`
- `crates/rog-providers/src/kbd_backlight.rs`
- `crates/rog-providers/src/asusd.rs`
- `crates/rog-providers/src/supergfx.rs`

When documenting a new feature, always note:

- whether it depends on system DBus or sysfs
- whether it can degrade to read-only
- whether the daemon or UI should surface a warning when it is unavailable
