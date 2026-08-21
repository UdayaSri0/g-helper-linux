# GUI Spec

This document describes the current GUI implemented by `crates/rog-ui/src/main.rs` and its `shell`, `theme`, `widgets`, and `fan_widgets` modules. Planned features that are not yet in the UI are called out separately.

## Scope

- The UI runs unprivileged.
- The UI does not perform hardware I/O directly.
- Hardware-affecting actions are routed through `rog-helperd` on the session bus.
- The UI is capability-driven and should degrade gracefully when dependencies or permissions are missing.

## Current Window Structure

The current top-level structure uses:

- `adw::ToolbarView`
- `adw::ViewStack`
- `adw::ToastOverlay`
- a compact 202 px left navigation sidebar with symbolic icons, hover/focus treatment, and a strong selected state
- a compact header with an application mark, title/subtitle identity, and daemon connection status
- vertically scrollable page containers clamped at 1260 px
- long pages, including Settings, scroll vertically instead of contributing their full content height to the top-level minimum

The default window is 1180 x 800. It does not impose a desktop-sized minimum: the visible page supplies its natural minimum, and flow-box groups reflow through wide, medium, and narrow layouts as space changes.

Shared presentation rules:

- hero cards are reserved for primary temperature, charge, and memory summaries
- compact cards carry secondary telemetry without arbitrary fixed heights
- semantic chips consistently distinguish available, informational, attention, and unavailable states
- long capability, endpoint, and metadata values wrap instead of being silently truncated
- page content uses restrained 20 px vertical rhythm and native GTK/Adwaita controls

The implemented page set is:

1. Dashboard
2. CPU
3. GPU
4. Battery
5. Memory
6. Lighting
7. Cooling
8. Setup & Access
9. Settings
10. Diagnostics
11. About

The tray menu currently supports:

- open main window
- open GPU page
- profile selection
- GPU mode selection
- About
- Quit

The window close button defaults to hiding the main window when tray support is
available. The tray Quit action and the Settings-page lifecycle Quit action remain
the explicit full-exit paths.

## Current Update Model

The current UI behavior is polling-based:

- a background Tokio runtime polls daemon state once per second
- a GTK timeout refreshes widgets every 250 ms from cached state
- one-shot tray/window actions and responsive breakpoint checks remain prompt, while bulk widget rendering runs only when the cached UI revision changes

This matches the current daemon polling model and is the expected behavior unless the architecture changes.

## Current Pages

### Dashboard

Purpose:

- quick system overview
- quick control surface
- first place for warnings and daemon-availability status

Current content:

- a responsive three-card hero row for CPU, GPU, and System State
- CPU temperature, utilisation, average clock, semantic status, and a compact 60-second utilisation sparkline
- GPU temperature, telemetry state, current mode when reported, and real core/memory clocks when available
- a System State summary for power source, battery state, daemon connection, available control count, and missing-service setup count
- a compact Current System Mode strip for known power, profile, GPU mode, cooling mode, and battery state values; unknown controls show an em dash rather than dependency errors
- compact Battery, Cooling, Memory, Power, and conditional NVMe cards
- a compact warning notification that reserves error emphasis for daemon/telemetry failure
- a compact two-column Quick Performance tile grid for:
  - performance profile
  - GPU mode
  - battery charge limit
  - keyboard backlight brightness
- a single compact `N control areas need setup` notification whose Review action opens Setup & Access; Dashboard does not duplicate remediation prose
- a content-driven Cooling Snapshot with CPU/GPU/NVMe thermal values and up to four detected fan RPM rows using static fan symbols; it ends after its last detected row
- a Live Performance panel with compact CPU usage, CPU temperature, GPU temperature, and memory usage trends
- a System Health panel summarizing `rog-helperd`, `asusd`, `supergfxd`, CPU writes, Lighting writes, fan-control state, and warning count with semantic indicators
- raw temperatures, fan endpoints, and other technical names under a collapsed Advanced Sensor Details section

Current behavior:

- controls are enabled or disabled based on daemon-reported capabilities
- optional capability gaps use a compact notification; daemon disconnection remains a prominent error
- Dashboard quick-action rows stay visible when relevant support is missing and point to the single dependency hint
- quick-action controls use compact dashboard tiles with aligned controls and apply buttons so disabled/read-only states still look intentional
- fan telemetry presentation is dynamic: the fan card and detail rows adapt to the detected `fan_rows` set rather than assuming a fixed one-fan or two-fan layout
- when a fan input is detected but has no current RPM value, the Dashboard keeps the row visible and shows it as unavailable instead of hiding it
- CPU, GPU, Battery, Cooling, and Memory cards provide keyboard-accessible navigation with visible hover/focus feedback
- System Overview uses five cards across at wide widths, three at medium widths, and one at narrow widths, preventing accidental 4+1 packing
- the remaining major `GtkFlowBox` groups transition together through wide, medium, and narrow modes without horizontal scrolling; Current System Mode uses 5-wide, 3+2, or a single stacked column
- GPU-temperature and memory histories are UI-only 60-sample buffers populated from the existing one-second daemon snapshots; they add no polling or hardware access
- Settings can hide Advanced System Health, the conditional NVMe card, or Cooling Snapshot and can enable a compact vertical layout

### Setup & Access

Purpose:

- explain why a control is unavailable without asking users to interpret raw diagnostics
- separate missing services, permission limits, unsupported hardware, and transient API failures

Current content and behavior:

- grouped Session Daemon, Administrator Access, Control Services, Permissions, Hardware Support, and Recommended Next Steps sections
- verified readiness for `rog-helperd`, `asusd`, `supergfxd`, UPower, and relevant `nvidia-smi` support
- live API checks are authoritative; executable and systemd-unit discovery is advanced evidence
- explicit available, administrator-required, authorized, denied, read-only, missing-backend/helper, external-service, unsupported, and unavailable language
- Refresh checks, Copy setup diagnostics, Copy privilege diagnostics, and Open full Diagnostics actions
- sysfs paths and endpoint details remain collapsed under Advanced details
- all checks are read-only; the UI does not invoke `sudo`, install packages, change permissions, or execute remediation commands
- opening or refreshing this page never prompts; only a meaningful `Unlock & Apply` operation may invoke PolicyKit

### CPU

Purpose:

- expose CPU telemetry and generic Linux CPU controls

Current content:

- overview cards for temperature, usage, package power, and average clock
- native filled-area Cairo history graphs for CPU usage and temperature, fed from existing cached history buffers and labelled with current/minimum/maximum values over the last 60 seconds
- compact CPU control-capability panel when writes are blocked or partially unavailable
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

- an overview for temperature plus optional NVIDIA utilisation, VRAM, power, and clock telemetry
- compact temperature and utilisation history graphs shown after real samples arrive
- one control-dependencies group for:
  - `asusd` performance profiles
  - `supergfxd` GPU modes
  - GPU switch hint
  - NVIDIA telemetry provider status
- controls group for:
  - profile apply
  - GPU mode apply
- status group showing the last action result

Current behavior:

- controls are gated by daemon capabilities
- NVIDIA data is read-only, daemon-polled every three seconds, and remains absent per field when unsupported
- reboot or logout requirement is shown through a capability hint from the daemon
- the dependency values distinguish missing `asusd`, missing `supergfxd`, unsupported hardware, and temporarily unavailable backend reads instead of falling back to vague `(n/a)` text
- unavailable control rows refer back to the single dependency summary instead of repeating the service explanation
- current-state value rows wrap long reason text instead of clipping, and apply rows use the same aligned control/button treatment as the Dashboard

### Battery

Purpose:

- battery and power telemetry view

Current content:

- large charge card with state, source/AC context, time estimate, and progress bar
- capability-gated battery charge-limit control
- compact health, cycle, active power, time, and source cards

The Dashboard retains a charge-limit quick control, while Battery is the full-control location. Both use the existing daemon action path.

### Memory

Purpose:

- memory and swap telemetry view

Current content:

- prominent memory-used card and progress bar with used and total values
- immediate cache, shared, buffers, and swap cards
- total, used, available, free, anonymous memory, swap totals, and activity in a collapsed detail disclosure
- zram and zswap state
- PSI memory pressure metrics and the detailed kernel memory breakdown in a collapsed Advanced section
- compact top-memory-users table with Process, PID, User, RAM, and Swap columns plus copy-to-clipboard support

### Lighting

Purpose:

- surface current keyboard lighting backend and controls

Current content:

- device/backend and explicit RGB, ARGB/multi-zone, and per-key capability rows
- local keyboard preview; it never writes hardware while the user edits controls
- one read-only/unavailable capability banner
- mode combo box only when `supports_modes` and non-empty `supported_modes` are reported
- brightness slider only when `supports_brightness` is reported
- RGB colour control only when `supports_rgb` is reported
- secondary colour, speed, direction, and zone rows only when the selected mode/backend reports them
- apply action
- collapsed backend details containing device, backend, capabilities, brightness, mode, and RGB values

Current implementation note:

- backend order is verified asusd, allow-listed native G615JMR target HID, sysfs brightness, unavailable;
  an active asusd owner suppresses native HID
- the sysfs keyboard backlight backend remains the brightness-only fallback
- sysfs daemon-reported supported modes are `Off` and `Static`
- the native G615JMR target is single-target RGB, not ARGB: it exposes no zones or per-key controls
- native effect Apply may request PolicyKit and reports accepted without readback; opening the page does not authenticate
- all mode, secondary colour, speed, direction, and zone visibility comes from daemon capability data
- Diagnostics copy includes a dedicated `Keyboard Lighting / RGB Diagnostics` section with sysfs paths, asusd DBus probe results, fallback reasons, and recommended actions
- unavailable or read-only states should use the same capability-aware wording style as the Dashboard and GPU page rather than raw `(n/a)` placeholders

### Settings

Purpose:

- keep application preferences separate from project metadata and hardware action pages
- expose the durable configuration surface without implying startup automation

Current content:

- Startup & Tray: On Close, Launch on Login, Start Minimized to Tray, and Exit Completely
- Dashboard: Advanced System Health, conditional NVMe card, Cooling Snapshot, and compact spacing
- Control Preferences: preferred charge limit and last manual performance profile
- Automation: an explicit notice that saved hardware preferences are not auto-applied
- Reset: destructive-style button with confirmation that restores rog-helper defaults

Current behavior:

- changes are sent to the daemon and atomically saved in versioned XDG `config.toml`
- a failed save leaves the previous good file in place and reports an error toast
- Reset synchronizes rog-helper's managed autostart entry with the default lifecycle setting
- remembered hardware values do not trigger writes on application or daemon startup

### Diagnostics

Purpose:

- expose capability and environment diagnostics without requiring a terminal

Current content:

- overview cards for daemon state, available control groups, and warning count
- structured Services, Permissions, Sensors, and Warnings groups before technical detail
- session daemon endpoint summary
- capability flags
- structured feature-access status and reason fields from daemon capabilities
- raw endpoint strings
- notes
- fan telemetry mapping, including chosen display label, hwmon device, raw sysfs input path, and current RPM or unavailable state
- warnings
- collapsed raw technical report with copy diagnostics button

### About

Purpose:

- provide a built-in app identity and runtime metadata view

Current content:

- leading identity card with packaged icon, product title/subtitle, version badge, and concise description
- binary
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
- manual `Check for Updates` action
- best-effort `Update Now` / `Download Latest` action
- source, support, and issue-reporting buttons

Current implementation note:

- version, license, authors, and repository URL come from Cargo manifest fields
- the UI still falls back to project defaults if those fields are blank
- maintainer GitHub metadata is derived from the repository URL when possible
- release checks are manual, run in the UI process, and use the GitHub Releases API
- automatic in-place replacement is limited to matching user-local direct-binary installs; unsupported installs fall back to opening the latest release page
- lifecycle and durable preferences are intentionally located on Settings

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

## Cooling Page

Cooling is the user-facing name of the existing internal `fans` page and backend API surface.

- uses the standard page header followed by compact backend, detected fan count, current mode, and mapping warning pills
- includes large side-by-side CPU/GPU temperature gauges when space allows, with operating MHz lines where telemetry is available
- includes animated fan rotors whose visual speed is scaled from live RPM, capped for readability, refreshed at 20 FPS only while mapped, and reduced when GTK animations are disabled
- keeps per-fan telemetry visible for every detected fan, including read-only fans
- shows individual fan cards with RPM, ID, backend, control support, endpoint details, notes, and warnings
- exposes manual percentage control only when the daemon reports writable manual percent support
- exposes sync mode only when more than one controllable fan is detected
- exposes time-limited full-speed boost buttons for 5, 10, and 15 minutes when boost is supported
- always exposes Return to Auto for controllable fans
- asks for explicit acknowledgement before first manual fan control
- shows a disabled safe-curve preview when curves are unsupported
- keeps raw diagnostics collapsed by default while preserving Copy fan diagnostics
- keeps RPM target and curve behavior capability-driven; unsupported backends remain read-only with diagnostics
- never writes directly to sysfs or hardware from the UI

## Planned or Missing UI Features

These are not current pages or complete current UI features.

### Planned or future pages

- Profiles page

Related note:

- some controls associated with those planned areas currently live on the Dashboard or GPU page

### Planned or future capabilities

- graphical fan-curve editor beyond the current validated DBus/API surface
- auto mode / rules editor
- exported diagnostics bundle
- more specialized conflict detection and service-management UI

## Maintenance Note

This file is intended to describe the real current UI. If `crates/rog-ui/src/` changes, update this document at the same time rather than leaving planned and current behavior mixed together.
