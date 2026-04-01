# UI Pages

This document describes the current pages implemented in `crates/rog-ui/src/main.rs`.

For each page, it lists:

- what the page shows
- what actions it supports
- what depends on capabilities
- what is still missing or planned

## Dashboard

### What it shows

- CPU temperature
- GPU temperature
- battery percentage
- power source
- fan summary
- NVMe temperature when detected
- warning banner and warning summary
- expandable detail groups for temperatures, fans, and endpoint samples

### What it supports

- performance profile quick actions
- GPU mode quick action
- battery charge-limit apply action
- keyboard backlight brightness apply action

### Capability dependencies

- profile controls depend on `has_profiles`
- GPU mode depends on `has_gpu_modes`
- charge limit depends on `has_charge_limit`
- keyboard backlight depends on `has_kbd_backlight` and backend writability
- fan telemetry depends on the dynamic `fan_rows` set from daemon telemetry; the UI does not assume a fixed fan count

Current behavior:

- disabled quick actions explain whether the block is caused by a missing backend, unsupported hardware, temporary backend unavailability, or permissions
- quick-action controls use consistent inline spacing and compact apply buttons so enabled and disabled states read cleanly on first launch

### Missing or planned

- no fan-curve controls yet
- no standalone auto-mode controls yet
- no richer profile rules editor yet

## CPU

### What it shows

- CPU temperature
- CPU usage
- CPU package power when available
- average clock
- CPU access-status banner when writes are blocked or partially unavailable
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
- disabled or read-only controls stay visible with short reason text instead of looking broken
- logical CPU toggle rows show the logical CPU id plus topology context, and the confirmation dialog spells out that the change can affect responsiveness and thermals

### Capability dependencies

- control availability depends on `cpu_caps`
- write controls depend on per-control entries in `cpu_caps.control_access`
- `policy_writable` remains a coarse any-writable summary
- logical CPU toggle support depends on `has_core_online` plus `core_online` access state

### Missing or planned

- no persistent CPU profile storage
- no graph widgets yet even though history buffers are collected
- no specialized per-vendor CPU policy backend

## GPU

### What it shows

- current ASUS performance profile
- current GPU mode
- GPU switch hint
- support/status summary

### What it supports

- performance profile apply
- GPU mode apply

### Capability dependencies

- profile controls depend on `has_profiles`
- GPU mode controls depend on `has_gpu_modes`
- reboot/logout hint depends on `requires_reboot_for_gpu_switch`

Current behavior:

- current-state rows and control subtitles now replace vague `(n/a)` or `Unavailable` placeholders with short reason text when `asusd` or `supergfxd` is missing, unsupported, or temporarily unavailable
- current-state value labels wrap longer reason strings instead of clipping, and the control rows use consistent aligned selector/apply layouts

### Missing or planned

- no advanced safe-switch workflow beyond current hints
- no richer “busy/unsafe” model yet

## Battery

### What it shows

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

### What it supports

- currently telemetry only

### Capability dependencies

- telemetry is best-effort and depends on `UPower` plus sysfs fallback

### Missing or planned

- no charge-limit setter on this page yet
- no battery presets or tradeoff explainer on this page yet

Important note:

- charge-limit control currently lives on the Dashboard

## RAM

### What it shows

- total, used, available, free, cached, buffers, shared, and anonymous memory
- swap totals and activity
- zram usage
- zswap state
- PSI memory pressure
- advanced memory breakdown
- top memory users

### What it supports

- copy top-memory-users summary

### Capability dependencies

- depends on procfs and related kernel support

### Missing or planned

- no historical graphs yet
- no export flow beyond clipboard copy

## Lighting

### What it shows

- current lighting backend
- current brightness
- current mode
- last action status

### What it supports

- mode selection
- brightness change
- apply action
- RGB picker placeholder

### Capability dependencies

- depends on the daemon exposing a lighting backend
- write support depends on backend writability
- RGB support depends on `supports_rgb`

### Missing or planned

- no Aura / RGB runtime backend yet
- current backend is keyboard brightness only
- current daemon-supported modes are limited to `Off` and `Static`

## Diagnostics

### What it shows

- troubleshooting summary for current support and permission blockers
- daemon endpoint summary
- capability matrix
- structured feature-access status and reason fields
- endpoint list
- notes
- fan telemetry diagnostics, including detected hwmon device, raw `fan*_input` path, chosen display label, and current RPM or unavailable state
- warnings
- CPU write-access report, including blocked sysfs paths and suggested checks

### What it supports

- copy diagnostics

### Capability dependencies

- diagnostics page is always intended to be available
- content depends on daemon reachability and current daemon state

### Missing or planned

- no file export yet
- no full diagnostics bundle API yet
- no historical event log yet

## About

### What it shows

- app name
- binary name
- version
- license
- session DBus API endpoint
- maintainer / contributor text
- source URL

### What it supports

- informational display only

### Capability dependencies

- none

### Missing or planned

- no packaged icon asset display
- current metadata still depends partly on Cargo fallback values

## Current Planned-But-Not-Implemented Pages

These are not separate pages in the current UI:

- Profiles
- Fans
- Settings

Related note:

- some controls associated with those planned areas already exist on the Dashboard, GPU page, or CPU page

When those pages are implemented, update both this file and `docs/GUI_SPEC.md`.
