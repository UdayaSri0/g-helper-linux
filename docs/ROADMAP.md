# Roadmap and Labels

This is the working roadmap for `rog-helper` (tray + window + user daemon) with suggested GitHub
issue labels so work stays organized.

## Current State

- Milestone 1 (read-only foundation): implemented.
  - `UPower` (system DBus): power source + battery percent (best-effort).
  - `hwmon` (sysfs): CPU temps and fan RPMs when exposed.
  - NVIDIA GPU temp fallback (read-only): `nvidia-smi` query when `hwmon` lacks dGPU temps.
  - Session DBus daemon: `GetCaps`, `GetState`, `GetTelemetry` (`a{sv}`).
  - UI: Dashboard + Lighting (kbd brightness) + Diagnostics + tray (SNI).

## Milestones (Engineering Goals)

### Milestone 2: `asusd` Integration (Profiles/Battery/Lighting)

Goal: implement the ASUS provider layer against real introspection output.

Tasks:

- Run Phase 0 discovery commands and commit the output to `docs/DBUS_NOTES.md` (or a new
  `docs/INTROSPECTION/<hostname>-<date>.md` file).
- Implement `AsusctlProvider` in `rog-providers`:
  - probe caps and endpoints
  - profile get/set (capability gated)
  - battery charge limit get/set (capability gated)
  - lighting get/set (capability gated)
- Extend daemon session API:
  - `SetProfile`, `SetBatteryLimit`, `SetLighting`
- UI:
  - Profiles page (switch + auto rules editor skeleton)
  - Battery page (limit presets + slider)
  - Lighting page (brightness + mode selector)

### Milestone 3: `supergfxd` Integration (GPU Mode Switching)

Goal: safe GPU mode switching with clear UX for "requires logout/reboot".

Tasks:

- Add `SupergfxProvider` in `rog-providers`:
  - get current mode
  - set mode
  - "requires logout/reboot" hint if the API provides it (or heuristic)
  - `can_switch_now()` best-effort (busy/unsafe hint)
- Extend daemon session API:
  - `SetGpuMode`
- UI:
  - GPU page + tray GPU mode submenu
  - Safe confirmation UX when switch is risky

### Milestone 4: Fan Curves + Automation Polish

Goal: fan curve editor (only when supported) + robust Auto Mode policy engine.

Tasks:

- Implement `FanProvider` curve support (only if exposed by `asusd`/model).
- Add curve presets + validation and "Reset/Revert" in UI.
- Implement daemon Auto Mode state machine:
  - debounce
  - manual override pause
  - AC/Battery rules applying profile/gpu/lighting/limit
- UI:
  - Auto Mode toggle + rules editor
  - Notifications on auto apply or delayed GPU switching

## Cross-Cutting Improvements (High Value)

- Telemetry:
  - add more sensor mapping heuristics (CPU package, GPU hot spot if available)
  - show per-fan RPM on Dashboard
  - consider NVML crate integration (read-only) behind a feature flag
- Diagnostics:
  - export "diagnostics bundle" (caps + warnings + recent events) to clipboard and file
  - include systemd service status checks for `asusd`/`supergfxd`
- Packaging:
  - systemd user unit + desktop entry install docs
  - optional: Flatpak after DBus permissions model is validated

## Suggested GitHub Labels (Tags)

Type:

- `type:bug`
- `type:feature`
- `type:chore`
- `type:docs`

Area:

- `area:ui`
- `area:daemon`
- `area:providers`
- `area:core`
- `area:packaging`
- `area:ci`

Feature:

- `feature:telemetry`
- `feature:profiles`
- `feature:fans`
- `feature:gpuswitch`
- `feature:battery`
- `feature:lighting`
- `feature:automation`
- `feature:diagnostics`

Priority:

- `prio:P0` (blocker for next milestone)
- `prio:P1` (important)
- `prio:P2` (nice-to-have)

State:

- `status:blocked`
- `status:needs-hw-logs`
- `status:needs-design`

Community:

- `good first issue`
- `help wanted`
