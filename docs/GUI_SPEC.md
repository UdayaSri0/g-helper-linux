# GUI Spec

This document describes the current GUI as implemented in `crates/rog-ui/src/main.rs`. Planned features that are not yet in the UI are called out separately so this file can serve both as a current UI reference and as a guide for future work.

## Scope

- The UI runs unprivileged.
- The UI does not perform hardware I/O directly.
- Hardware-affecting actions are routed through `rog-helperd` on the session bus.
- The UI is capability-driven and should degrade gracefully when dependencies or permissions are missing.

## Current Window Structure

The current top-level structure uses:

- `adw::ToolbarView`
- `adw::ViewStack`
- `adw::ViewSwitcher`
- `adw::ToastOverlay`

The implemented page set is:

1. Dashboard
2. CPU
3. GPU
4. Battery
5. RAM
6. Lighting
7. Diagnostics
8. About

The tray menu currently supports:

- open main window
- open GPU page
- profile selection
- GPU mode selection
- About
- Quit

The window close button defaults to hiding the main window when tray support is
available. The tray Quit action and the About-page lifecycle Quit action remain
the explicit full-exit paths.

## Current Update Model

The current UI behavior is polling-based:

- a background Tokio runtime polls daemon state once per second
- a GTK timeout refreshes widgets every 250 ms from cached state

This matches the current daemon polling model and is the expected behavior unless the architecture changes.

## Current Pages

### Dashboard

Purpose:

- quick system overview
- quick control surface
- first place for warnings and daemon-availability status

Current content:

- metric cards for CPU temperature, GPU temperature, battery, power source, fans, and NVMe temperature when available
- warning banner and warning summary area
- quick actions for:
  - performance profile
  - GPU mode
  - battery charge limit
  - keyboard backlight brightness
- expandable detail sections for temperatures, fans, and endpoint snippets

Current behavior:

- controls are enabled or disabled based on daemon-reported capabilities
- warning area shows concise reason text for missing services, unsupported features, and permission-blocked controls
- Dashboard quick-action rows stay visible when relevant support is missing and explain why the control is disabled
- quick-action controls use aligned inline control clusters with compact apply buttons so disabled/read-only states still look intentional
- fan telemetry presentation is dynamic: the fan card and detail rows adapt to the detected `fan_rows` set rather than assuming a fixed one-fan or two-fan layout
- when a fan input is detected but has no current RPM value, the Dashboard keeps the row visible and shows it as unavailable instead of hiding it

### CPU

Purpose:

- expose CPU telemetry and generic Linux CPU controls

Current content:

- overview cards for temperature, usage, package power, and average clock
- CPU access-status banner when writes are blocked or partially unavailable
- quick controls for:
  - turbo boost
  - power mode
  - min frequency
  - max frequency
- policy section for:
  - scaling driver
  - physical core count
  - logical thread count
  - governor
  - energy performance preference
- logical CPU / thread table
- advanced section for:
  - logical CPU online/offline toggles
  - CPU sysfs access report with readable/writable paths
  - copy CPU diagnostics

Current behavior:

- CPU controls can be visible but read-only
- write access is gated per control from daemon-reported CPU access status
- blocked write paths are surfaced in diagnostics instead of a fake repair action
- quick-control and policy rows show current-value or read-only subtitles so the user can tell what is actionable at a glance
- logical CPU toggles use two-line rows with topology context and require explicit confirmation in the current UI
- the main CPU row table is ordered by logical CPU id and distinguishes physical-core count from logical-thread rows

### GPU

Purpose:

- surface GPU mode and related control state

Current content:

- current state group showing:
  - current ASUS performance profile
  - current GPU mode
  - GPU switch hint
- controls group for:
  - profile apply
  - GPU mode apply
- status group showing the last action result

Current behavior:

- controls are gated by daemon capabilities
- reboot or logout requirement is shown through a capability hint from the daemon
- the current-state values and control subtitles distinguish missing `asusd`, missing `supergfxd`, unsupported hardware, and temporarily unavailable backend reads instead of falling back to vague `(n/a)` text
- current-state value rows wrap long reason text instead of clipping, and apply rows use the same aligned control/button treatment as the Dashboard

### Battery

Purpose:

- battery and power telemetry view

Current content:

- charge percentage
- battery state
- power source
- AC online
- charge power
- discharge power
- time to full
- time to empty
- battery health
- cycle count

Important note:

- the current Battery page is telemetry-focused
- the current battery charge-limit control is on the Dashboard, not on this page

### RAM

Purpose:

- memory and swap telemetry view

Current content:

- total, used, available, free, cached, buffers, shared, and anonymous memory
- swap totals and activity
- zram and zswap state
- PSI memory pressure metrics
- advanced memory breakdown
- top memory users with copy-to-clipboard support

### Lighting

Purpose:

- surface current keyboard lighting backend and controls

Current content:

- backend and device name
- current brightness
- current mode
- current RGB colour when reported
- availability status
- mode combo box
- brightness slider
- RGB colour control
- apply action
- last-action status

Current implementation note:

- the daemon prefers an asusd Aura/RGB DBus backend when introspection confirms one exists
- the sysfs keyboard backlight backend remains the brightness-only fallback
- sysfs daemon-reported supported modes are `Off` and `Static`
- Aura mode choices and RGB enablement come from the daemon-reported backend capability data
- Diagnostics copy includes a dedicated `Keyboard Lighting / RGB Diagnostics` section with sysfs paths, asusd DBus probe results, fallback reasons, and recommended actions
- unavailable or read-only states should use the same capability-aware wording style as the Dashboard and GPU page rather than raw `(n/a)` placeholders

### Diagnostics

Purpose:

- expose capability and environment diagnostics without requiring a terminal

Current content:

- troubleshooting summary for the current machine state
- session daemon endpoint summary
- capability flags
- structured feature-access status and reason fields from daemon capabilities
- raw endpoint strings
- notes
- fan telemetry mapping, including chosen display label, hwmon device, raw sysfs input path, and current RPM or unavailable state
- warnings
- copy diagnostics button

### About

Purpose:

- provide a built-in app identity and runtime metadata view

Current content:

- name
- binary
- version
- license
- session DBus API endpoint
- authors field
- repository URL
- maintainer name
- maintainer GitHub URL
- latest-release status
- last check timestamp
- last check result
- release-notes preview
- lifecycle controls for:
  - close behavior
  - launch on login
  - start minimized to tray
  - explicit full exit
- manual `Check for Updates` action
- best-effort `Update Now` / `Download Latest` action
- source, support, and issue-reporting buttons

Current implementation note:

- version, license, authors, and repository URL come from Cargo manifest fields
- the UI still falls back to project defaults if those fields are blank
- maintainer GitHub metadata is derived from the repository URL when possible
- release checks are manual, run in the UI process, and use the GitHub Releases API
- automatic in-place replacement is limited to matching user-local direct-binary installs; unsupported installs fall back to opening the latest release page
- lifecycle preferences are limited UI settings persisted in the user's XDG config/autostart locations; they are not a general hardware-control settings system

## Current Capability Behavior

The current UI behavior is driven by daemon-reported capabilities and daemon-reported state.

Examples:

- profile controls depend on `has_profiles`
- GPU controls depend on `has_gpu_modes`
- battery charge-limit controls depend on `has_charge_limit`
- keyboard backlight controls depend on `has_kbd_backlight` and backend writability
- CPU controls depend on `cpu_caps`, with per-control write gating from `control_access`

This is the expected current behavior and is preferable to guessing based on hardware model names.

## Error and Warning UX

Current UI expectations:

- no raw stack traces in normal UI flow
- warnings should be surfaced as readable status text or banners
- unavailable features should degrade to disabled or read-only UI
- diagnostics should remain copyable

Current examples in the implementation include:

- daemon unavailable messaging
- keyboard backlight read-only banner
- CPU write-access banner with diagnostics handoff
- last-action status for lighting and GPU/profile actions

## Fans Page

The Fans page is a current UI page.

- shows fan backend, detected fan count, current mode, last action, and a warning banner for read-only or uncertain states
- keeps per-fan telemetry visible for every detected fan
- exposes manual percentage control only when the daemon reports writable manual percent support
- exposes sync mode only when more than one controllable fan is detected
- exposes time-limited full-speed boost buttons for 5, 10, and 15 minutes when boost is supported
- always exposes Return to Auto for controllable fans
- asks for explicit acknowledgement before first manual fan control
- keeps RPM target and curve behavior capability-driven; unsupported backends remain read-only with diagnostics
- never writes directly to sysfs or hardware from the UI

## Planned or Missing UI Features

These are not current pages or complete current UI features.

### Planned or future pages

- Profiles page
- Settings page

Related note:

- some controls associated with those planned areas currently live on the Dashboard or GPU page

### Planned or future capabilities

- graphical fan-curve editor beyond the current validated DBus/API surface
- auto mode / rules editor
- exported diagnostics bundle
- more specialized conflict detection and service-management UI

## Maintenance Note

This file is intended to describe the real current UI. If the implementation in `crates/rog-ui/src/main.rs` changes, update this document at the same time rather than leaving planned and current behavior mixed together.
