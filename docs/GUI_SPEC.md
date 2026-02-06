# GUI Spec

This document describes the target GUI (tray + window) for `rog-helper-ui` and how it should evolve
as new providers become available.

## Scope

- The UI must run unprivileged (no root).
- The UI must not perform hardware I/O directly.
- All hardware-affecting actions must go through `rog-helperd` (session DBus) which in turn talks to
  system services (`asusd`, `supergfxd`) or read-only sources (`UPower`, `hwmon`).
- The UI must be capability-driven: if a feature is unsupported or a dependency is missing, show a
  clear read-only state.

## Safety Model (Important)

The UI does not "control CPU/GPU temperature" directly.
Instead, it exposes safe controls that influence thermals:

- Performance profile (Silent/Balanced/Turbo) via `asusd` (or safe fallback only when explicitly
  supported).
- Fan presets and fan curves (only when the machine exposes a supported interface).
- GPU mode switching (Integrated/Hybrid/Dedicated) via `supergfxd`.

Temperature is treated as telemetry. Any "temperature control" language in the UI should be framed
as "cooling/performance settings" rather than direct temperature setpoints.

## Main Window Structure

Use `adw::ToolbarView` + `adw::ViewStack` + `adw::ViewSwitcher` for top navigation.

Do not call `gtk_window_set_titlebar()` on `AdwApplicationWindow` (it is unsupported).

Pages:

1. Dashboard (telemetry + quick status)
2. Profiles (performance profiles + Auto rules editor)
3. Fans (capability-gated; presets + curves)
4. GPU (capability-gated; mode switching with safe UX)
5. Battery (capability-gated; charge limit presets + slider)
6. Lighting (capability-gated; brightness + modes)
7. Settings (service enablement + logging + export diagnostics)
8. Diagnostics (always available; debug/health view)

## Dashboard (Milestone 1+)

Primary goals:

- Show "at a glance" telemetry: CPU temp, GPU temp, fan RPMs, battery %, power source.
- Make failure states obvious and actionable (daemon not running, missing dependencies, etc).

Recommended layout:

- A compact grid of metric cards (CPU temp, GPU temp, Battery, Power Source).
- A short fan RPM summary line (when fan readings exist).
- An expandable "Sensors" section that lists all discovered temperature and fan sensors.
- A status area for daemon connectivity and high-level warnings.

Update behavior:

- UI updates on a GTK timer (ex: 250ms) using the latest snapshot stored in shared state.
- Telemetry fetch runs off the main thread (Tokio runtime in a background thread).
- Target refresh: 1 Hz telemetry from daemon (more is unnecessary and can cause DBus spam).

## Diagnostics (Milestone 1+)

Purpose:

- Make it easy to answer "why is feature X missing" without asking the user to open a terminal.

Must show:

- Session DBus API endpoint (name, path, interface).
- `DeviceCaps` matrix (true/false).
- Raw discovery endpoints (system bus names, sysfs hints).
- Notes and warnings.

Must provide:

- "Copy diagnostics" button (clipboard).
- Later: "Export diagnostics bundle" to a file.

## Profiles Page (Milestone 2)

When `has_profiles=true`:

- Buttons/toggles for Silent/Balanced/Turbo.
- Show current active profile.
- Auto rules editor (AC vs Battery):
  - On AC: profile + GPU mode + lighting option
  - On Battery: same
- Apply + Save actions.

When unsupported:

- Show read-only explanation and dependency hint (ex: `asusd` missing).

## Fans Page (Milestone 4, capability-gated)

When `has_fan_curves=false`:

- Show "fan curves unsupported" message.
- Still show fan RPMs if `has_fan_reading=true`.

When supported:

- Curve editor with validation feedback.
- Presets: Quiet / Balanced / Performance / Safe Max Cooling.
- Apply + Revert + Reset.
- "Advanced" toggle gates risky options.

## GPU Page (Milestone 3)

When `has_gpu_modes=true`:

- Show current mode and brief explanation.
- Mode switch buttons.
- Busy/unsafe hint (best-effort).
- Clear messaging when logout/reboot required.

## Battery Page (Milestone 2)

When `has_charge_limit=true`:

- Presets: 60/80/100.
- Slider for supported range (default clamp 40..=100).
- Explain battery health tradeoffs.

## Lighting Page (Milestone 2)

When `has_aura=true` or `has_kbd_backlight=true`:

- Brightness slider (capability-driven range).
- Mode dropdown (capability-driven).
- "Disable on battery" option (ties into Auto rules).

## Settings (Milestone 2+)

- Enable `rog-helperd` on login (systemd --user).
- Logging level.
- Export diagnostics bundle.
- Conflict detection (warn if other fan control services are active).

## Error and Warning UX (Non-negotiable)

- Never show raw stack traces in the UI.
- Always show:
  - Human-friendly message
  - Suggested action
  - "Copy technical details"

