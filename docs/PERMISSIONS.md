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
- it is the only application component that calls the optional root helper

## Privileged Helper Trust Boundary

Packaged installs include `rog-helper-privileged`, an on-demand root service on the system bus.
The UI remains unprivileged and never connects to it directly. The application architecture is:

```text
GTK UI -> session DBus -> rog-helperd -> system DBus -> rog-helper-privileged
```

Its system identity is `io.github.roghelper.Privileged`, object
`/io/github/roghelper/Privileged`, interface `io.github.roghelper.Privileged1`. It exposes discovery,
a non-interactive allow-listed `CanPerform` diagnostic probe, and explicit CPU, verified fan, and
keyboard-brightness operations. Lighting privilege is limited to the canonical ASUS WMI keyboard
LED; it does not expose RGB, HID, USB, GPU, or generic system writes.

The helper has no generic file, sysfs, command, program, argument, or shell API. Its methods
accept a narrowly defined domain operation, validate it, select a fixed endpoint internally,
authorize the D-Bus caller, and return a sanitized domain error. Raw root/backend errors must not
cross the boundary.

The PolicyKit actions are:

- `io.github.roghelper.cpu.control`
- `io.github.roghelper.fans.control`
- `io.github.roghelper.lighting.control`
- `io.github.roghelper.system.configure`

There is deliberately no generic “root” permission. Interactive authorization for control
methods is delegated to the desktop PolicyKit agent; the GTK application must not ask for or
handle a password.

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
- any authorization enforced by `supergfxd` or its system-service policy

Current effect of failure:

- GPU mode controls are unavailable in the UI
- diagnostics should still show the missing capability
- missing `supergfxd` is reported as a missing external backend, not as a request for root access

Architecture decision:

- GPU telemetry remains unprivileged (`hwmon` and optional `nvidia-smi`)
- GPU mode discovery, validation, switching, and transition safety remain authoritative in `supergfxd`
- `rog-helper-privileged` deliberately exposes no GPU method
- ROG Helper does not bind or unbind PCI devices, write GPU sysfs mode files, reload kernel
  modules, power-cycle GPUs, issue raw ACPI calls, or execute commands for GPU switching

“Privileged control” is therefore subsystem-specific. It does not mean every hardware operation
belongs in ROG Helper's root helper. A mature external system service remains the correct boundary
when it already owns the hardware operation and its safety policy.

### CPU controls

Backend:

- `crates/rog-providers/src/cpu.rs`

Depends on:

- readable CPU sysfs controls, with either direct write permission or the packaged privileged helper

Examples:

- scaling governor
- EPP
- boost toggle
- min/max frequency limits
- per-logical-CPU online state

Current effect of failure:

- CPU telemetry can still work
- `policy_writable` may become false as a coarse summary
- daemon diagnostics report direct and privileged write routes plus authorization state per control
- supported permission-blocked controls stay actionable and request PolicyKit authorization only on Apply
- denied or failed writes refresh actual state and never stop telemetry

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

The repository ships a typed PolicyKit CPU fallback. Existing direct provider behavior remains the
first choice; the helper is called only after a supported operation fails for write permission.

That means:

- keyboard brightness may be readable but not writable
- CPU controls may require administrator authorization when direct writes are blocked
- behavior depends on the current distro, kernel, udev rules, and file ownership
- the app does not change ownership, modes, udev rules, or sysfs permissions

This is expected behavior in the current design.

## Setup & Access Checks

The Setup & Access page and `rog-helper setup-check` make these boundaries visible without
changing them. They perform read-only service/API probes and filesystem access checks. Binary
presence and systemd state are diagnostic evidence only; a dependency is marked ready only after
its expected API responds.

These checks never run `sudo`, install packages, modify sysfs permissions, create udev rules, or
bypass system DBus authorization. The privileged-status probe only asks PolicyKit for a
non-interactive decision. Remediation falls back to installation and
troubleshooting documentation when the repository has no verified distro-specific command.

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
- telemetry available but direct writes not allowed -> UI shows authorization-required, helper-missing, or read-only state plus diagnostics
- backend missing -> warnings and diagnostics expose the condition
- session daemon missing -> UI surfaces daemon connectivity problems

This is the intended current behavior and should be preserved when adding new backends.

## What the Repository Does Not Currently Provide

The repository does not currently include:

- bundled udev rules
- a custom permission broker
- privileged RGB/HID/USB, GPU, or generic sysfs write methods

`rog-helper-privileged` is a narrowly scoped root service, not a privileged replacement for the
session daemon. It exits after an idle timeout and is optional on unsupported distributions.

## Practical Guidance

When a feature is visible but not writable, inspect:

- `docs/TROUBLESHOOTING.md`
- `crates/rog-providers/src/cpu.rs`
- `crates/rog-providers/src/kbd_backlight.rs`
- `crates/rog-providers/src/asusd.rs`
- `crates/rog-providers/src/supergfx.rs`

### Keyboard lighting manual validation

On supervised ASUS hardware, verify the detected `max_brightness` and test the existing Apply flow
at `0`, the minimum visible level (`1` on the canonical three-level ASUS WMI LED), a middle level,
and the reported maximum. After every write, confirm the daemon reports the same readback and the UI
continues normal operation. Repeat with direct sysfs access, with the helper installed, with PolicyKit
denied/cancelled, and with the helper unavailable. RGB must remain disabled unless the verified asusd
Aura API reports it writable.

When documenting a new feature, always note:

- whether it depends on system DBus or sysfs
- whether it can degrade to read-only
- whether the daemon or UI should surface a warning when it is unavailable
