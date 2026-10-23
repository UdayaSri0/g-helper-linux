# Permissions

## Capability and per-operation access

Detection is not authorization. Every control surface must keep these dimensions separate:

- `feature_supported`: a verified backend implements the requested operation
- `mapping_verified`: the operation targets a known semantic device/channel
- `telemetry_available`: a fresh numeric read succeeded; numeric zero is valid telemetry
- `direct_writable`: the unprivileged daemon can write the validated endpoint directly
- `privileged_helper_ready`: a compatible typed helper, PolicyKit, and operation category are ready
- `authorization_state`: `not_checked`, `required`, `authorized`, `denied`, or `unavailable` for that operation
- `can_apply`: the selected operation has a safe direct or typed privileged route
- `reason`: a stable reason code, with explanatory UI text layered above it

Opening or refreshing Setup & Access performs only non-authorizing probes. It must not produce a
PolicyKit dialog. Authentication is requested only by an explicit Apply action. A CPU authorization
probe does not authorize battery, fan, or lighting operations; each domain retains its own last
operation state. `Auto / BIOS Default` describes firmware control mode, not access permission.

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
a non-interactive allow-listed `CanPerform` diagnostic probe, and explicit CPU, verified fan,
keyboard-brightness, native Aura-effect, and standard battery-threshold operations. Privileged API
v3 is current; v2 introduced the high-level `SetAuraEffect` operation for the exact G615JMR target
contract, and v3 adds marker-gated fan Auto recovery. `RecoverFansIfArmed` performs the dedicated
`fans.recover` PolicyKit check without user interaction; the packaged policy permits it only for
an active local session. It can only restore exact semantic IDs recorded in the root-owned marker.
Battery privilege is limited to one unambiguous
`type=Battery` power-supply device exposing the documented `charge_control_end_threshold` ABI. It
does not expose generic HID/USB writes, caller-selected paths, raw bytes, report IDs, command IDs,
zones, GPU operations, or generic system writes.

The helper has no generic file, sysfs, command, program, argument, or shell API. Its methods
accept a narrowly defined domain operation, validate it, select a fixed endpoint internally,
authorize the D-Bus caller, and return a sanitized domain error. Raw root/backend errors must not
cross the boundary.

The PolicyKit actions are:

- `io.github.roghelper.cpu.control`
- `io.github.roghelper.battery.control`
- `io.github.roghelper.fans.control`
- `io.github.roghelper.fans.recover`
- `io.github.roghelper.lighting.control`

There is deliberately no generic “root” permission. Interactive authorization for control
methods is delegated to the desktop PolicyKit agent; the GTK application must not ask for or
handle a password. The packaged policy uses `auth_admin` rather than `auth_admin_keep` for writes.
The separate `fans.recover` action allows Auto-only recovery from an active local session without
a prompt; the helper still requires its exact root-owned marker and cannot enable a curve through
that action.

The complete root method/resource inventory, filesystem checks, hardening decisions, and residual
risks are recorded in [PRIVILEGED_SECURITY_REVIEW.md](PRIVILEGED_SECURITY_REVIEW.md).

The developer-only `rog-helper lighting-test --safe-sequence` flow creates no new privilege route.
It is a read-only preflight unless the exact G615JMR confirmation flag is supplied, and then sends a
fixed list of semantic requests to the existing session-daemon `SetLighting` method. Only the daemon
may call the typed helper, and the existing `io.github.roghelper.lighting.control` PolicyKit action,
identity/ownership checks, root-only alias, duplicate suppression, rate limit, and fixed report encoder
remain authoritative.

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

- profile control remains unavailable because asusd is authoritative for profiles
- charge-limit control checks the standard kernel power-supply fallback only when asusd is absent,
  unreachable, or does not expose the feature
- a permission-blocked standard threshold may use the typed battery helper; an absent or ambiguous
  threshold remains unsupported/read-only

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

- verified asusd API, a user-writable LED brightness entry, or the approved ASUS LED helper route

Current effect of failure:

- brightness can still be readable
- current backend remains visible
- UI shows the control as read-only or reports a write failure

### ASUS Aura RGB

Priority and permission behavior:

- the exact asusd 6.3.8-6.4.0 Aura contract is preferred and follows asusd's system-service policy
- otherwise, the exact G615JMR target HID contract may use the lighting PolicyKit action
- the UI and session daemon remain unprivileged; authorization is requested only when Apply invokes
  the native write
- the packaged udev rule creates root-only `/dev/rog-helper-aura`; it does not add `MODE`, `GROUP`,
  `TAG+=uaccess`, or an ACL
- the helper revalidates the alias and opened device against DMI, `0b05:19b6`, interface `00`, the
  `asus` driver, descriptor hash, and 64-byte report contract before writing
- a running ASUS daemon suppresses native HID to prevent competing owners
- no reliable hardware effect readback exists, so a completed write is reported as accepted without readback

### Battery charge-limit fallback

Backend:

- `crates/rog-providers/src/power_supply.rs`

Rules:

- asusd remains the first choice
- the fallback uses Linux's documented `charge_control_end_threshold` power-supply ABI
- discovery requires an exact present `type=Battery` device and rejects ambiguous multi-battery
  write targets
- callers provide only a percentage; the helper discovers the endpoint internally
- requests use the shared 20..=100 application contract and the actual value is read back because
  kernel drivers may round thresholds
- telemetry and detection never request authentication
- the dedicated action is `io.github.roghelper.battery.control`

Reference: [Linux power-supply class ABI](https://www.kernel.org/doc/Documentation/ABI/testing/sysfs-class-power).

## Consolidated control / privilege matrix

The same decisions are exported by `GetCaps` as `control_privilege_matrix`. Each structured row
contains operation, support, backend, access, privilege requirement, existing system daemon,
direct-user-write state, fallback suitability, risk, and implementation decision.

| Operation | Current backend / priority | Privilege? | Existing daemon owns privilege? | Direct user write | Helper fallback appropriate? | Risk | Decision |
| --- | --- | --- | --- | --- | --- | --- | --- |
| CPU boost, governor, EPP, frequency limits, core online, power mode | validated CPU sysfs | Sometimes | No | Preferred when permitted | Yes, typed operation after permission denial | Medium; core online high | Keep telemetry unprivileged; validate choices, bounds, core IDs, and readback |
| GPU mode | `supergfxd` | External service policy | Yes, `supergfxd` | No | No | High | Keep supergfxd authoritative; no PCI/module/ACPI/direct-mode helper methods |
| Battery telemetry | `UPower` + power-supply sysfs | No | UPower for part of telemetry | Read-only | No | Low | Always unprivileged |
| Battery charge limit | `asusd`, then standard power-supply threshold | Sometimes | Prefer `asusd` | Yes when kernel attribute permits | Yes only for one validated standard threshold | Medium | Never replace working asusd; validate 20..=100 and return readback |
| Keyboard brightness | verified asusd, LED sysfs, approved ASUS LED helper | Sometimes | Prefer `asusd` | Preferred | Yes for canonical approved LED only | Low | No caller path and no generic lighting write |
| Aura/RGB/modes | verified asusd, then allow-listed G615JMR target HID | External service policy or PolicyKit | Prefer `asusd` | No direct user hidraw access | Yes, only the path-free high-level Aura effect operation | High | Suppress HID when asusd owns Aura; revalidate a root-only alias; never accept paths or bytes |
| Fan RPM | hwmon | No | No | Read-only | No | Low | Telemetry remains unprivileged |
| Fan curve/Auto | verified ASUS WMI hwmon ABI | Sometimes | No | Preferred when permitted | Yes only for verified endpoints | High | Reject generic PWM/RPM-target guesses; preserve Auto restore safety |
| Fan manual percent/RPM target/boost | generic hwmon candidates only | Not authorized | No | Deliberately unused | No | High | Compatibility methods return unsupported; no privileged method exists |
| Performance profile | `asusd` | External service policy | Yes, `asusd` | No | No | Medium | Keep asusd authoritative |
| Persistent configuration | per-user XDG file | No | No | Yes | No | Low | Atomic user-session write only |
| Login/autostart integration | per-user desktop/XDG integration | No | No | Yes | No | Low | Never route through the root helper |

## Sysfs Write Limitations

The repository ships typed PolicyKit fallbacks for narrowly approved CPU, fan, keyboard LED,
native Aura, and battery-threshold writes. Existing direct/provider behavior remains the first choice; the helper is
called only after a supported operation fails for write permission.

That means:

- keyboard brightness and a standard battery threshold may be readable but not directly writable
- CPU controls may require administrator authorization when direct writes are blocked
- behavior depends on the current distro, kernel, udev rules, and file ownership
- the app does not change ownership, modes, or sysfs permissions; native packages install one
  root-only target-specific Aura alias rule

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

`rog-helper privileged-status` also performs local read-only installation checks for the expected
binary, systemd unit, system-D-Bus activation/policy, PolicyKit policy, narrow udev rule, Aura
alias, descriptor, and protocol. Development checkouts can install only the current privileged
integration with `packaging/scripts/install-dev-privileged.sh`; that script may use `sudo`, but the
UI and session daemon must continue to run as the desktop user.

## System DBus Expectations

The repository assumes these service categories may exist:

- `UPower`
- `asusd`
- `supergfxd`

The current code handles absence by degrading capabilities rather than treating them as fatal startup errors.

However:

- the daemon will still be less useful without them
- profile and GPU control depend on their dedicated external services
- battery-limit control prefers asusd and can use the documented standard kernel fallback when safe

## Read-Only Degradation Behavior

The current implementation prefers graceful degradation over hidden failure.

Current examples:

- feature capability is false -> UI disables or hides the control
- telemetry available but direct writes not allowed -> UI distinguishes helper installation,
  reachability, compatibility, PolicyKit, category, and write-path readiness; `not_checked` means
  authentication on Apply rather than read-only
- backend missing -> warnings and diagnostics expose the condition
- session daemon missing -> UI surfaces daemon connectivity problems

This is the intended current behavior and should be preserved when adding new backends.

## What the Repository Does Not Currently Provide

The repository does not provide a custom permission broker, generic privileged HID/USB or sysfs
writes, or a GPU helper method. Its one bundled Aura udev rule is an exact root-only alias, not a
permission grant to the desktop user.

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
denied/cancelled, and with the helper unavailable. RGB must remain disabled unless a verified asusd
or exact native Aura backend reports it. Native RGB needs separate supervised physical validation;
an `accepted_no_readback` result is not confirmation by itself.

When documenting a new feature, always note:

- whether it depends on system DBus or sysfs
- whether it can degrade to read-only
- whether the daemon or UI should surface a warning when it is unavailable
