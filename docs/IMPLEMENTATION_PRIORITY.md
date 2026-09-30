# Implementation Priority

Baseline: `0145a261dcbfe57bea452c3256d3b8caef49ff0b` (v0.3.1)  
Prepared: 2026-09-30

This backlog is ordered by safety dependencies and user value, not novelty. A stage may move forward
only when its acceptance gate is satisfied. “Implemented in code” never substitutes for supervised
hardware evidence where a physical effect has no reliable readback.

## Dependency graph

```text
P0 source truth and target validation
├── documentation/support taxonomy
├── fan-sync capability correction
├── installed helper API-v2/package readiness
├── G615JMR Aura physical validation
└── ASUS WMI fan/Auto/recovery validation
     │
     v
P1 persistent named profiles
├── versioned schema and migration
├── capability-backed semantic settings
├── ordered apply/readback/rollback
└── validated fan-curve persistence only
     │
     v
P1 AC/battery policy runtime
├── explicit opt-in and manual override
├── debounce/idempotence/reconciliation
└── profile transaction reuse
     │
     v
P2 Linux-native convenience
├── display refresh discovery/control
├── policy-driven display changes
├── global shortcuts to typed actions
└── keyboard timeout through session idle APIs
     │
     v
P3 broader devices and peripherals
└── identity -> fixtures -> diagnostics -> narrow write -> physical evidence
```

## P0 — Hardware correctness, safety, and truthful capabilities

### P0.1 Reconcile source truth

- Keep `README.md`, feature/provider matrices, roadmap, architecture, discovery documents, security
  review, hardware support, and release checklists consistent with source.
- Use these distinct labels: software-validated, hardware-validated, backend-limited,
  hardware-unvalidated, diagnostic-only, partial, missing, and deliberately unsupported.
- Never list generic PWM, RPM target, or boost as pending physical validation. They have no approved
  backend and are deliberately unsupported.

Acceptance gate:

- Source and current docs agree on every feature in the baseline audit.
- Exact upstream revisions and licensing roles are recorded.
- Hardware matrices cannot be read as support claims without a committed machine record.

### P0.2 Correct fan-sync capability semantics

- Prevent curve-only fans from advertising an operational sync feature while sync affects only
  manual-percent routing.
- Either disable the capability for all current backends or design a separate, explicit multi-curve
  transaction. Do not silently loop independent writes without rollback.
- Preserve read-only fan rows and semantic fan IDs.

Acceptance gate:

- Capability, daemon behavior, UI state, and documentation agree.
- Tests cover one/multiple curve-only fans and prove that no unsupported manual write occurs.
- Any future multi-curve operation stages all values, enables safely, and restores every affected
  channel to Auto after a partial failure.

### P0.3 Validate installed privileged integration

- Install the current API-v2 helper and current systemd, system-D-Bus, PolicyKit, and udev payload on
  the target through a normal package/development-install path.
- Confirm diagnostics distinguish missing, unreachable, incompatible, denied, cancelled, and
  unsupported states.
- Retain root ownership, narrow device alias, no `uaccess`, and current systemd sandbox.

Acceptance gate:

- `rog-helper privileged-status` reports the current binary/API and every expected integration file.
- Denial/cancellation changes no hardware state and telemetry remains available.
- No caller-controlled path, command, environment, or raw packet crosses the root boundary.

### P0.4 Supervised G615JMR Aura validation

- Observe every advertised mode on the physical LEDs.
- Exercise every exposed speed/direction combination and Breathe secondary color.
- Test PolicyKit success, denial, cancellation, helper absence, timeout/failure, duplicate/rate
  suppression, active-asusd suppression, and suspend/resume.
- Record transport acceptance separately from physical observation.

Acceptance gate:

- A completed machine record contains identity/descriptor evidence and before/after observations.
- Unsupported ARGB/zones/per-key controls remain hidden.
- Any failed or ambiguous combination is removed from capability advertisement.

### P0.5 Supervised ASUS WMI fan validation

- Validate CPU, GPU, and Mid channels independently with thermal supervision.
- Confirm all eight point readbacks, enable state, RPM response, and explicit Auto restoration.
- Test malformed input, disappearing endpoints, partial failure, PolicyKit denial/cancel, daemon
  restart, helper clean stop, helper kill/restart marker recovery, suspend/resume, and firmware/
  platform-profile ownership changes.

Acceptance gate:

- A committed target record passes the complete matrix for every advertised channel.
- Any partial failure restores all affected channels to Auto.
- Profile changes cannot leave stale curves represented as active.
- Only then may persistent fan profiles or automatic fan policy depend on the backend.

## P1 — Persistent profiles, presets, and policy automation

### P1.1 Named hardware profiles

- Add a versioned schema for named profiles containing only semantic, capability-backed settings.
- Keep unsupported fields explicit; do not store sysfs paths, hidraw paths, or raw payloads.
- Define deterministic apply order. A profile change precedes its fan curves because ASUS profile
  writes invalidate custom curves.
- Read back each supported setting. On partial failure, return a structured result and restore fan
  Auto where fan ownership may be uncertain.
- Do not auto-apply profiles merely because they were saved.

Acceptance gate:

- Migration, malformed/unknown fields, atomic save, rename/delete, and capability mismatch tests pass.
- Apply ordering, idempotence, readback, partial failure, and rollback are deterministic.
- Fan curves cannot be persisted or applied until P0.5 is satisfied.
- Native Aura settings cannot be promoted as validated profile content until P0.4 is satisfied.

### P1.2 Presets

- Provide conservative product presets only as ordinary named-profile templates.
- Show exactly which settings are supported, skipped, or blocked on the current machine.
- Never claim a preset is a vendor firmware default unless read from an authoritative backend.

Acceptance gate:

- Applying a preset uses the same transaction and validation path as any named profile.
- Unsupported settings produce explicit skips, not optimistic UI success.

### P1.3 AC/battery policy engine

- Wire the existing policy model into daemon runtime with UPower/logind events.
- Persist explicit AC and battery rules, default automation off, and expose pause/resume/manual
  override.
- Debounce source changes, make repeated events idempotent, and reconcile actual state after failure
  or service restart.
- Reuse named-profile transactions; do not create a second hardware-application path.

Acceptance gate:

- Tests cover rapid AC/battery flapping, startup, resume, daemon restart, manual override, unavailable
  providers, partial profile apply, and recovery.
- No hardware write occurs at boot/login until the user explicitly enables automation.
- A failed fan action restores Auto and pauses unsafe reapplication.

## P2 — Linux-native convenience and ergonomics

### P2.1 Display refresh controls

- Begin with read-only connector/mode/current-refresh discovery.
- Choose a session-scoped compositor, portal, or DRM contract with clear ownership; avoid arbitrary
  shell commands and never add display operations to the root helper merely for convenience.
- Handle multiple displays, missing internal panels, unsupported sessions, and mode disappearance.
- Add AC/battery refresh automation only after the P1 policy runtime is stable.

Acceptance gate:

- Fixtures/integration tests cover multiple connectors and unsupported Wayland/X11 sessions.
- Apply has readback or an authoritative compositor result and cannot target an inferred display.
- Failure leaves the previous mode active and visible in diagnostics.

### P2.2 Global shortcuts and ASUS hotkeys

- Inventory standard Linux input keycodes and desktop shortcut/portal support read-only.
- Map shortcuts only to existing typed daemon actions such as profile cycling or opening a page.
- Keep desktop registration/session ownership separate from hardware providers.

Acceptance gate:

- No arbitrary command binding or root input listener exists.
- Conflicts, unsupported desktops, and repeat behavior are explicit.
- Every action is capability-gated and tested without manufacturing success.

### P2.3 Keyboard timeout

- Use desktop/session idle APIs with lock, suspend, and resume awareness; do not globally snoop input.
- Preserve the user’s configured brightness and restore only state owned by the feature.
- Keep AC and battery timeout values separate and allow explicit disablement.

Acceptance gate:

- The selected session API is documented and tested across idle/active/lock/suspend transitions.
- Backend loss cannot leave brightness permanently forced or claim unsupported control.

### P2.4 Update integrity and diagnostics export

- Prefer distro package managers and `fwupd` for system-managed updates.
- If direct user-local replacement remains, verify a published checksum or signature before rename.
- Add an explicitly redacted, read-only diagnostics bundle.

Acceptance gate:

- A corrupt/mismatched download is rejected without replacing the executable.
- Diagnostics export contains no arbitrary file collection or secret-bearing bus data.

## P3 — Broader hardware and peripheral expansion

For each new laptop, Aura device, fan backend, or peripheral:

1. Collect read-only DMI, USB/HID, driver, descriptor, sysfs, and service-contract evidence.
2. Cross-check protocol/ABI facts with kernel or compatible Linux sources.
3. Add deterministic fixtures and diagnostics.
4. Add a narrow capability entry and typed semantic operation only after review.
5. Suppress native access when an authoritative daemon owns the endpoint.
6. Perform supervised physical validation and commit a machine/device record.

Acceptance gate:

- Exact identity and ABI are allow-listed; similarity to G615JMR is insufficient.
- No raw writer, generic path, broad permission, or silent ownership conflict is introduced.
- The support matrix remains “implemented but hardware-unvalidated” until the physical record exists.

## Explicit blockers and non-goals

- Generic writable-looking `pwmN`, `pwmN_enable`, and `fanN_target` files do not authorize control.
- Windows ACPI `DeviceIoControl`, service/elevation, display, hotkey, and updater APIs are not portable
  backends and will not be translated.
- G-Helper GPL source will not be copied or mechanically ported.
- Direct/software Aura effects, external keyboards, calibration, and hysteresis remain P3 research
  unless independent Linux evidence establishes a bounded safe contract.
- No release/tag or support-level promotion occurs as part of these audit stages.

## Recommended next stage

Execute P0.2 through P0.5: correct the fan-sync capability mismatch, validate the installed API-v2
integration, then perform the separately supervised Aura and ASUS WMI fan matrices. Persistent
profiles and automation should wait for those gates rather than guessing at hardware behavior.
