# UI Pages

This document describes the current pages implemented in `crates/rog-ui/src/`.

All ten pages live in one `adw::ViewStack` selected by the persistent left sidebar. They share the same 1260 px responsive content container, page-title hierarchy, compact/standard/hero metric tiers, semantic status chips, neutral card surfaces, spacing, and capability-state language.

For each page, it lists:

- what the page shows
- what actions it supports
- what depends on capabilities
- what is still missing or planned

## Dashboard

### What it shows

- CPU hero with temperature, utilisation, average clock, status, and cached 60-second utilisation sparkline
- GPU hero with temperature, telemetry availability, current mode, and real clocks when reported
- System State hero with power, battery, connection, control availability, and setup-issue count
- Current System Mode strip for known power, profile, GPU mode, cooling mode, and battery state
- compact battery, cooling, memory, power, and conditional NVMe cards
- Cooling Snapshot with a three-temperature strip and up to four static live-RPM fan rows
- compact CPU usage/temperature, GPU temperature, and memory history trends
- System Health summary for daemon/dependency/CPU/Lighting/fan/warning state with semantic indicators and a Setup & Access action
- compact warning notification and warning summary
- collapsed Advanced Sensor Details for raw temperatures, fans, and endpoint samples

### What it supports

- performance profile quick actions
- GPU mode quick action
- battery charge-limit apply action
- keyboard backlight brightness apply action
- keyboard and pointer navigation from CPU, GPU, Battery, Cooling, and Memory cards to their detailed pages

### Capability dependencies

- profile controls depend on `has_profiles`
- GPU mode depends on `has_gpu_modes`
- charge limit depends on `has_charge_limit`
- keyboard backlight depends on `has_kbd_backlight` and backend writability
- fan telemetry depends on the dynamic `fan_rows` set from daemon telemetry; the UI does not assume a fixed fan count

Current behavior:

- disabled quick actions remain capability-driven; remediation guidance lives on Setup & Access rather than being duplicated below Dashboard controls
- Quick Performance uses a two-column dashboard tile grid where space permits and wraps without changing capability gating
- System Overview uses five columns at wide widths, an intentional 3+2 arrangement at medium widths, and one column at narrow widths; it never falls into a 4+1 layout
- Cooling Snapshot is content-driven and ends directly after its last detected fan row
- Current System Mode uses five columns when wide, 3+2 packing at medium widths, and one column at narrow widths
- primary metrics, quick controls, cockpit panels, live trends, and system health share the same cached responsive mode so they reflow only when a breakpoint changes
- CPU, GPU-temperature, and memory histories are bounded to 60 cached samples and never poll from a draw callback
- highly prominent warning styling is reserved for daemon disconnection; optional capability gaps use compact warning language

### Missing or planned

- Dashboard intentionally does not duplicate Cooling fan curves or other detailed-page controls
- GPU utilisation, VRAM use, SMART health, and other unreported metrics remain absent rather than simulated
- no richer profile rules editor yet

## Setup & Access

### What it shows

- live readiness for `rog-helperd`, `asusd`, `supergfxd`, UPower, and `nvidia-smi` when relevant
- the controls each service enables and whether the expected API actually responded
- an Administrator Access section for the privileged helper, system DBus, PolicyKit, CPU, fans, lighting, GPU switching, and battery-limit ownership
- reusable access language: Available, Administrator access required, Authorized, Authorization denied, Read only, Backend missing, Privileged helper missing, Unsupported hardware, Setup required, and External service required
- CPU policy, keyboard-lighting, and fan-control write states as Writable, Read-only, Unsupported, Unavailable, or Checking
- a hardware-support summary that preserves unsupported states instead of treating them as permission failures
- human-readable next steps without unverified package-manager commands
- technical DBus, systemd, binary, and sysfs evidence under collapsed Advanced details

### What it supports

- Refresh checks
- Copy setup diagnostics
- Copy privilege diagnostics, without exposing raw privileged endpoint paths
- Open full Diagnostics

### Safety and discovery behavior

- checks are unprivileged and read-only
- opening or refreshing the page never requests authorization; a PolicyKit prompt can start only from a meaningful Apply action
- there is no global root or permanent unlock mode; authorization stays scoped to the operation category
- shipped PolicyKit actions do not retain authorization; every privileged write performs its category check
- service readiness requires a working DBus/API query; binary presence and systemd state are supporting evidence only
- the page never runs `sudo`, installs packages, changes sysfs modes, or bypasses DBus authorization
- Dashboard Review actions navigate here and show only a compact issue count

## CPU

### What it shows

- CPU temperature
- CPU usage
- CPU package power when available
- average clock
- filled-area CPU usage and temperature history graphs backed by existing cached samples, with current/min/max and 60-second time labels
- compact CPU control-capability panel when writes are blocked or partially unavailable
- scaling driver
- physical core count
- logical thread count
- quick-control row subtitles that show current turbo, preset, and frequency-limit context when readable
- logical CPU / thread state table
- detected CPU sysfs access report with readable/writable paths
- CPU diagnostics copy action

### What it supports

- turbo boost toggle
- CPU power mode quick control
- min frequency control
- max frequency control
- governor apply
- EPP apply
- per-logical-CPU online/offline toggle

Current behavior:

- quick CPU controls are staged in grouped apply rows so the scope of each Apply button is clear
- a group Apply action is labelled `Unlock & Apply` when one of its selected controls requires administrator authorization
- disabled or read-only controls stay visible with short reason text instead of looking broken
- logical CPU toggle rows show the logical CPU id plus topology context, and the confirmation dialog spells out that the change can affect responsiveness and thermals

### Capability dependencies

- control availability depends on `cpu_caps`
- write controls depend on per-control entries in `cpu_caps.control_access`
- `policy_writable` remains a coarse any-writable summary
- logical CPU toggle support depends on `has_core_online` plus `core_online` access state

### Missing or planned

- no persistent CPU profile storage
- no clock or package-power history graph yet
- no specialized per-vendor CPU policy backend

## GPU

### What it shows

- current ASUS performance profile
- current GPU mode
- GPU temperature when available
- optional NVIDIA utilisation, VRAM usage, power, core clock, and memory clock
- responsive overview cards and real-sample temperature/utilisation history graphs
- one consolidated dependency group for `asusd`, `supergfxd`, NVIDIA telemetry status, and the GPU switch hint
- support/status summary

### What it supports

- performance profile apply
- GPU mode apply

### Capability dependencies

- profile controls depend on `has_profiles`
- GPU mode controls depend on `has_gpu_modes`
- selectors are populated from `gpu_supported_modes` reported by supergfxd
- pending/logout/reboot/unsafe UX depends on `gpu_switch_state` and `gpu_switch_hint`

Current behavior:

- the dependency group replaces vague `(n/a)` placeholders with short reason text when `asusd` or `supergfxd` is missing, unsupported, or temporarily unavailable
- unavailable selectors refer to the consolidated dependency group, while all telemetry remains read-only and independent
- NVIDIA metrics stay absent rather than showing invented zeroes when the GPU, driver, command, or individual field is unavailable
- missing supergfxd is described as a missing external service, never as a request to run the UI or daemon as root

### Safety boundary

- supergfxd remains authoritative for available modes, external authorization, and switching
- another switch is disabled while supergfxd reports a pending, logout, reboot, or unsafe transition
- telemetry remains available independently of the switching service

## Battery

### What it shows

- a charge hero with percentage, state, AC/source context, time estimate, and progress
- a compact row for health, cycles, active power, time, and source

### What it supports

- battery charge-limit changes through the existing daemon/asusd path
- `Unlock & Apply` only when the validated kernel fallback requires administrator authorization

### Capability dependencies

- telemetry is best-effort and depends on `UPower` plus sysfs fallback
- charge-limit write access depends on daemon-reported `charge_limit_access`

### Missing or planned

- no battery presets or tradeoff explainer on this page yet

## Memory

### What it shows

- prominent usage progress display with used and total values
- immediate cache, shared, buffers, and swap summaries
- total, used, available, free, anonymous memory, and swap detail in a collapsed disclosure
- swap totals and activity
- zram usage
- zswap state
- PSI memory pressure and advanced kernel breakdown in a collapsed Advanced section
- top memory users in Process/PID/User/RAM/Swap columns

### What it supports

- copy top-memory-users summary

### Capability dependencies

- depends on procfs and related kernel support

### Missing or planned

- no historical graphs yet
- no export flow beyond clipboard copy

## Lighting

### What it shows

- ROG Keyboard status card with backend, RGB support, effect count, and control readiness
- keyboard-shaped local preview with variable key widths and an explicit Apply-only hardware-write boundary
- availability status
- last action status
- collapsed backend details for device, current backend, RGB/ARGB/per-key capability, brightness, mode, and colour

### What it supports

- mode selection only when `supports_modes` and backend modes are reported
- brightness change only when `supports_brightness` is reported
- strict `#RRGGBB` entry, GTK picker, and visual preset swatches only when the daemon reports `supports_rgb`
- secondary colour, speed, direction, and zones only when the active mode/backend reports them
- Current/Pending draft summaries, local Reset, and dirty-only Apply with applying/success/failure state

### Capability dependencies

- depends on the daemon exposing a lighting backend
- write support depends on backend writability
- helper installation, reachability, API compatibility, PolicyKit, lighting category, and write-path
  readiness are represented separately; `authorization=not_checked` remains editable
- RGB support depends on `supports_rgb`
- ARGB/multi-zone and per-key states use their own flags and are never inferred from RGB
- secondary colour, speed, direction, and zone controls are not shown without explicit capability
- unavailable states should explain whether the page is unsupported, read-only, or temporarily unavailable

### Backend behavior

- backend priority is exact asusd Aura, allow-listed native G615JMR target HID, sysfs brightness, unavailable
- active asusd ownership suppresses native HID to prevent competing writers
- sysfs keyboard backlight remains available as a brightness-only fallback
- sysfs-supported modes are limited to `Off` and `Static`
- Aura-supported modes, speeds, and zones are not invented by the UI; they come from daemon/backend reporting
- native HID authorization occurs only on Apply; its successful outcome is accepted without hardware readback

### Diagnostics report

- copy diagnostics includes a `Keyboard Lighting / RGB Diagnostics` section
- the report lists sysfs LED paths, current brightness, read/write status, services checked, interfaces found, candidate method/property signatures for brightness/mode/RGB/speed/zones, verified-contract state, fallback reasons, and recommended action text

## Settings

### What it shows

- Startup & Tray preferences: On Close, Launch on Login, Start Minimized to Tray, and full exit
- dashboard visibility switches for Advanced System Health, the conditional NVMe card, and Cooling Snapshot
- compact dashboard spacing
- an optional preferred battery charge limit and last manual performance profile
- an explicit Automation section explaining that startup hardware actions are disabled
- a confirmed Reset to Defaults action

### What it supports

- durable, versioned preferences through the daemon-owned XDG configuration
- immediate dashboard visibility/layout updates
- synchronization of the app-managed XDG autostart entry
- reset of rog-helper preferences without touching hardware state or unrelated user files

### Persistence and safety

- canonical file: `$XDG_CONFIG_HOME/rog-helper/config.toml`, or `$HOME/.config/rog-helper/config.toml`
- the UI reads startup behavior locally, then uses the daemon configuration API for writes
- preferred hardware values are reminders for manual use and are never auto-applied at boot/login
- malformed configuration falls back safely; unknown fields are tolerated; writes replace atomically
- legacy `rog-helper/ui.toml` is migrated when no canonical file exists and is left in place

## Diagnostics

### What it shows

- troubleshooting summary for current support and permission blockers
- overview cards for daemon connection, available control groups, and warning count
- structured Services, Permissions, Sensors, and Warnings groups
- daemon endpoint summary
- capability matrix
- structured feature-access status and reason fields
- endpoint list
- notes
- fan telemetry diagnostics, including detected hwmon device, raw `fan*_input` path, chosen display label, and current RPM or unavailable state
- warnings and a collapsed raw technical report
- CPU write-access report, including blocked sysfs paths and suggested checks

### What it supports

- copy diagnostics
- open Setup & Access for guided remediation

### Capability dependencies

- diagnostics page is always intended to be available
- content depends on daemon reachability and current daemon state

### Missing or planned

- no file export yet
- no full diagnostics bundle API yet
- no historical event log yet

## About

### What it shows

- leading identity card with packaged icon, app name/subtitle, version, and description
- binary name
- version
- license
- session DBus API endpoint
- authors
- repository URL
- maintainer name
- maintainer GitHub URL
- latest release version
- update status
- last check timestamp
- last check result
- release-notes preview

### What it supports

- manual `Check for Updates`
- best-effort `Update Now` for safe user-local direct-binary installs
- browser actions for source / GitHub, support / repository, and issue reporting
- safe fallback to opening the latest release page when automatic in-place replacement is not supported

### Capability dependencies

- static metadata has no daemon capability dependency
- release checks depend on network access to GitHub
- automatic in-place replacement depends on a matching direct release asset plus a writable user-local binary path

### Missing or planned

- no distro package-manager integration or privileged updater
- no scheduled background update checks
- lifecycle and control-preference settings live on the dedicated Settings page
- metadata comes from Cargo manifest fields and only falls back to project defaults if those fields are blank

## Cooling

Current page. The internal page identifier remains `fans` for compatibility.

- standard page header with compact backend/count/mode/mapping status pills
- large CPU/GPU temperature gauges that sit side by side on normal laptop widths and stack on narrow windows
- operating MHz display in the CPU/GPU gauge cards when telemetry is available
- animated fan rotors in the hero dashboard, scaled from live RPM telemetry, limited to 20 FPS, paused while unmapped, and reduced with GTK's animation setting
- capability warning banner
- sync toggle when multiple controllable fans exist
- manual percentage slider when writable PWM support is confirmed
- 5/10/15 minute boost actions when manual percentage support exists
- Return to Auto action
- disabled fan-curve preview when curve support is unavailable
- individual fan cards with RPM, backend, read-only/controllable badges, endpoint details, notes, and warnings
- copyable fan diagnostics with IDs, RPM, percentages, endpoints, notes, and warnings
- collapsed diagnostics section by default
- read-only telemetry remains visible when writes are unsupported or permission-denied

## Current Planned-But-Not-Implemented Pages

These are not separate pages in the current UI:

- Profiles

Related note:

- some controls associated with those planned areas already exist on the Dashboard, GPU page, or CPU page

When those pages are implemented, update both this file and `docs/GUI_SPEC.md`.
