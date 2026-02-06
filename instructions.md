ROLE
You are a senior Linux desktop engineer + systems integrator. Your job is to BUILD (not just describe) a Linux-native “G-Helper-like” control application for an ASUS ROG Strix laptop.

CONTEXT (NON-NEGOTIABLE)
- G-Helper is Windows-only and depends on Windows-only APIs/drivers. Do NOT attempt to “run it on Linux” or port WinForms code.
- On Linux, use the existing ASUS Linux ecosystem:
  - asusd/asusctl for: performance profiles, battery charge limit, Aura/RGB (where supported), and (sometimes) fan curves.
  - supergfxd/supergfxctl for: GPU mode switching (integrated/hybrid/dedicated; and other supported modes).
- UI must NOT require root. Automation must run as a user-level daemon (systemd --user).
- The app MUST capability-detect everything and gracefully degrade to read-only mode when a feature is unsupported.
- Safety-first: clamp inputs, provide “Reset to defaults”, avoid unsafe tuning, and avoid leaving system in unknown state on crash.

TARGET DEVICE (STARTING POINT)
- ASUS ROG Strix G16 (G615J-class). Real model may vary; capability detection is required.
- Assume NVIDIA dGPU present (e.g., RTX 4060). Do not hardcode vendor details; detect and adapt.

PROJECT NAME
- rog-helper (working name)

PRIMARY GOAL
A single, polished Linux control app (tray + window) providing:
- Performance profiles
- Fan controls (safe presets + custom fan curves if supported)
- GPU mode switching
- Battery charge limit
- RGB/Aura / keyboard lighting
- Telemetry dashboard
- Automation rules (AC vs Battery)
- Diagnostics and robust error handling

NON-GOALS (V1)
- Writing kernel drivers.
- Direct EC poking / HID raw writes unless there is no safe alternative (avoid this in V1).
- Undervolting/overclocking by default.
- “Works on every ASUS ever” claims. V1 targets a small set of machines with best-effort fallback.

===========================================================
TECH STACK (FIXED FOR THIS PROJECT)
===========================================================
Use Rust + GTK4 + libadwaita + DBus.

- UI: GTK4 + libadwaita (optionally relm4)
- DBus: zbus
- Async runtime: tokio
- Config: serde + toml
- Logging: tracing (+ journald friendly output)
- Notifications: notify-rust (or GTK notifications)
- Packaging: systemd user service + .desktop file + icon
- Optional telemetry enhancement: NVML via a Rust crate OR (as fallback) parse nvidia-smi (read-only)

===========================================================
REPO / WORKSPACE STRUCTURE (RUST WORKSPACE)
===========================================================
rog-helper/
  Cargo.toml
  crates/
    rog-core/         # domain types + validation + policy engine
    rog-providers/    # DBus clients + sysfs/hwmon + upower
    rog-daemon/       # rog-helperd: automation + state machine + session DBus API
    rog-ui/           # rog-helper-ui: GTK4/libadwaita GUI + tray
    rog-cli/          # diagnostics + probing tool for developers/users
  packaging/
    systemd-user/
      rog-helperd.service
    desktop/
      rog-helper.desktop
      icons/...
  docs/
    BUILD.md
    TROUBLESHOOTING.md
    ARCHITECTURE.md
    DBUS_NOTES.md

===========================================================
ARCHITECTURE (MUST FOLLOW)
===========================================================
Split into three layers:
1) UI (rog-helper-ui) — unprivileged
2) Daemon (rog-helperd) — unprivileged user service; policy/automation + error aggregation
3) Providers (rog-providers) — DBus clients + sysfs readers

Data flow:
UI  <— session DBus —>  Daemon  <— system DBus —>  asusd / supergfxd
                                       |
                                    sysfs/hwmon + UPower for telemetry

Never put hardware I/O logic directly in the UI.

===========================================================
PHASE 0 — REQUIRED DISCOVERY (STOP AND ASK FOR THIS FIRST)
===========================================================
Before implementing DBus calls, you MUST ask the human developer to paste the output of these commands from the target laptop:

1) Services on the system bus:
   busctl --system list | rg -i "asus|rog|supergfx|power|upower"

2) asusd status (name may vary by distro):
   systemctl status asusd

3) supergfxd status:
   systemctl status supergfxd

4) Introspection (use the bus names found in step 1):
   busctl --system introspect <asus.bus.name> <object/path>
   busctl --system introspect <supergfx.bus.name> <object/path>

5) Sensors (hwmon):
   ls /sys/class/hwmon
   grep -R . /sys/class/hwmon/hwmon*/name | head -n 50

6) Keyboard backlight (if relevant):
   ls /sys/class/leds | rg -i "kbd|asus"

RULE: Do NOT hardcode DBus interface names/paths until verified from introspection output.

===========================================================
DOMAIN MODEL (rog-core) — TYPES + VALIDATION
===========================================================
Implement these types (no IO code in rog-core):

- PowerSource = Ac | Battery
- PerformanceProfile = Silent | Balanced | Turbo | Custom(String)
- GpuMode = Integrated | Hybrid | Dedicated | (Other(String) for future)
- BatteryLimit = integer range (default clamp 40..=100; adjust if device exposes min/max)
- FanCurve:
    - points: Vec<{ temp_c: u8, duty_percent: u8 }>
    - domain: Cpu | Gpu | Mid | Other(String)
- Lighting:
    - brightness: 0..=3 or 0..=100 (capability-driven)
    - mode: Off | Static | Breathing | Rainbow | (Other(String))
    - optional: per-zone config (future)
- DeviceCaps:
    - has_profiles
    - has_fan_curves
    - has_fan_reading
    - has_charge_limit
    - has_gpu_modes
    - has_aura
    - has_kbd_backlight
    - requires_reboot_for_gpu_switch (capability/heuristic)
    - plus: any discovered endpoints as strings for diagnostics

VALIDATION RULES (MANDATORY)
- Fan curve validation:
  - temp range: default clamp 20..=100 (or device-provided)
  - duty_percent: 0..=100
  - points must be sorted by temp
  - enforce monotonic duty (optional but recommended; if not monotonic, warn and auto-fix)
  - enforce minimum safe floor (e.g., never allow 0% above a certain temp)
  - provide “Reset to defaults” + “Apply safe curve” actions
- Battery limit validation:
  - clamp to supported range; default 40..=100
- GPU mode switching:
  - guard if dGPU busy (best-effort heuristic)
  - never spam switching: debounce and require user confirmation if risky

===========================================================
PROVIDER LAYER (rog-providers) — TRAITS + IMPLEMENTATIONS
===========================================================
Define traits (interfaces) used by the daemon/policy:
- ProfileProvider: get_profile(), set_profile()
- FanProvider:
    - get_fan_rpm()
    - supports_curves()
    - get_curve(domain)
    - set_curve(domain, curve)
- GpuProvider: get_mode(), set_mode(), can_switch_now() -> hint/warning
- BatteryProvider: get_limit(), set_limit(), get_status()
- LightingProvider: get_state(), set_state()
- TelemetryProvider: get_snapshot()

Implement providers:
1) AsusctlProvider (DBus client to asusd)
   Must support (capability gated):
   - Get/set performance profile
   - Get/set battery charge limit
   - Get/set Aura/RGB and/or keyboard lighting
   - Fan curves IF model supports (otherwise return NotSupported)
   Must expose:
   - probe_caps() returning DeviceCaps + raw endpoint info for diagnostics

2) SupergfxProvider (DBus client to supergfxd)
   Must support:
   - Get current GPU mode
   - Set GPU mode
   - Report if reboot/logout needed
   - Provide “busy/unsafe” hint if available (or best-effort heuristic)

3) HwmonTelemetryProvider (sysfs)
   Must:
   - enumerate /sys/class/hwmon/hwmon*
   - map labels to CPU temp, GPU temp, fan RPM best-effort
   - return a TelemetrySnapshot:
       temps: cpu/gpu/other
       fans: rpm per domain
       timestamps

4) UPowerProvider (DBus)
   Must:
   - AC online state
   - battery percentage
   - energy rate / time-to-empty/full if available

OPTIONAL Provider:
5) NvidiaTelemetryProvider
   - Try NVML first; fallback to parsing nvidia-smi
   - Read-only, never required for app to function

===========================================================
ERROR HANDLING (MUST BE EXCELLENT)
===========================================================
Use typed errors across the entire stack:

Error categories:
- DependencyMissing(asusd/supergfxd/upower)
- NotSupported(feature)
- PermissionDenied(detail)
- InvalidInput(detail)
- TransientFailure(timeout/busy)
- Unexpected(detail)

UX rules:
- Never show raw stack traces in UI.
- UI must show:
  - Human-friendly message
  - Suggested action
  - “Copy technical details” (includes provider error chain and introspection summary)
- Diagnostics page must show last ~200 events/errors.

Logging rules:
- Use tracing with structured logs
- Include correlation IDs for user actions (e.g., “apply_profile#1234”)

Safety rules on failure:
- If apply fails mid-way (e.g., profile set succeeded but GPU mode failed), reflect partial state clearly and provide “Re-apply” and “Revert” actions.
- Avoid leaving the user in a “mystery state”.

===========================================================
DAEMON (rog-daemon) — STATE MACHINE + AUTOMATION
===========================================================
Daemon responsibilities:
- Expose a session-DBus API to the UI:
  - GetCaps
  - GetState (profile, gpu mode, battery limit, lighting, telemetry snapshot)
  - SetProfile
  - SetGpuMode
  - SetBatteryLimit
  - SetLighting
  - SetFanCurve (capability gated)
  - ToggleAutoMode
  - SetAutoRules
  - GetDiagnosticsBundle (text)
- Run automation state machine (“Auto Mode”):
  Inputs:
   - AC plug/unplug events (UPower)
   - battery percent thresholds (optional)
   - GPU busy heuristic (optional)
   - manual override toggle
  Outputs:
   - apply desired profile / gpu mode / lighting / battery limit depending on rules
  Mandatory features:
   - Debounce events (avoid rapid toggles)
   - Manual override: if user manually switches, auto pauses until re-enabled
   - “Switching guard”: if dGPU busy, delay and notify rather than hard-failing

Persistence:
- Store config in ~/.config/rog-helper/config.toml
Required config keys:
  [auto]
  enabled = true/false
  [auto.on_ac]
  profile = "Turbo"
  gpu_mode = "Hybrid"
  lighting = "On" / "Off" / mode
  [auto.on_battery]
  profile = "Silent"
  gpu_mode = "Integrated"
  lighting = "Off"
  [battery]
  limit = 80
  [fans]
  curves_enabled = true/false
  curve_cpu = [...]
  curve_gpu = [...]

===========================================================
UI (rog-ui) — UX SPEC DOWN TO DETAILS
===========================================================
Must implement:
A) TRAY MENU (fast actions)
- Show current status at top:
  - Profile: Silent/Balanced/Turbo
  - GPU Mode: Integrated/Hybrid/Dedicated
  - Battery: % + AC/Battery
  - Temps: CPU/GPU (if available)
- Quick actions:
  - Switch Profile (radio items)
  - Switch GPU Mode (radio items; disabled if NotSupported)
  - Battery Limit submenu (60/80/100 + custom)
  - Auto Mode toggle
  - Open Main Window
  - Quit

B) MAIN WINDOW PAGES (libadwaita)
1) Dashboard
   - Live tiles: CPU temp, GPU temp, Fan RPMs, Battery %, Power source
   - Current modes: Profile, GPU mode, Battery limit, Lighting
   - Warnings panel:
       - “supergfxd not running”
       - “asusd not running”
       - “fan curves unsupported”
       - “GPU switch requires logout/reboot”
2) Profiles
   - Buttons/toggles for Silent/Balanced/Turbo
   - Auto Mode rules editor:
     - On AC: profile dropdown + GPU mode dropdown + lighting option
     - On Battery: same
   - Apply + Save buttons
3) Fans (CAPABILITY-GATED)
   - If has_fan_curves=false: show “Your model does not expose fan curves via asusd; read-only mode.”
   - If supported:
     - Curve editor with points (temp, duty)
     - Graph view optional
     - Validation feedback inline
     - Presets: Quiet / Balanced / Performance / Safe Max Cooling
     - Apply + Revert + Reset to defaults
4) GPU
   - Current mode + explanation of each mode
   - Mode switch buttons
   - “Busy” indicator if dGPU in use
   - Clear messaging if reboot/logout required
5) Battery
   - Current limit
   - Slider + preset buttons (60/80/100)
   - “Healthy battery mode” explanation
6) Lighting (RGB/Aura)
   - Brightness slider
   - Mode dropdown (capability-driven)
   - Toggle: “Disable lighting on battery” (ties into auto rules)
7) Settings
   - Start daemon on login (systemd user enable)
   - Logging level
   - Export Diagnostics Bundle
   - “Conflicts” check (warn if another fan controller is running)

C) NOTIFICATIONS
- “Profile changed to Turbo”
- “GPU mode change scheduled (requires logout/reboot)” or “GPU mode change delayed (dGPU busy)”
- “Auto mode applied settings for AC/Battery”

D) DIAGNOSTICS PAGE (MUST-HAVE)
Show:
- Detected DBus endpoints (services + interfaces + object paths)
- Service status checks (asusd/supergfxd)
- DeviceCaps matrix
- Last errors and logs
- Copy/export button

===========================================================
CONFLICT / SAFETY CHECKS (MANDATORY)
===========================================================
- Detect and warn if multiple controllers may fight (examples: another fan-control service, rog-control-center, custom scripts).
- Default to safe presets; do not expose risky tuning unless user explicitly enables “Advanced”.
- Always provide Reset/Revert.

===========================================================
IMPLEMENTATION PLAN (MILESTONES)
===========================================================
Milestone 1 — Read-only foundation
- Create workspace + CI (fmt/clippy/tests)
- Implement UPowerProvider + HwmonTelemetryProvider
- Build UI shell + tray + Dashboard (read-only telemetry)
- Add Diagnostics page (service presence + caps “unknown” placeholders)

Milestone 2 — asusd integration
- Implement AsusctlProvider using DBus introspection outputs
- Implement caps probing
- Add Profiles, Battery, Lighting controls (capability gated)
- Add robust errors + notifications

Milestone 3 — supergfxd integration
- Implement SupergfxProvider
- GPU page + tray switching
- Safe switching UX and “busy” guard
- Auto rules for AC/Battery (profile + gpu mode)

Milestone 4 — Fan curves + automation polish
- Implement FanProvider curve support if available
- Curve editor + validation + presets + apply/revert
- Improve policy engine debouncing + manual override
- Import/export settings + docs

===========================================================
TESTING REQUIREMENTS
===========================================================
- Unit tests in rog-core:
  - fan curve validation (ordering, clamp, monotonic, safe floors)
  - policy state machine transitions (AC/Battery events, debounce, manual override)
- Provider tests:
  - mock providers for UI/daemon tests
  - at least one integration test that verifies DBus parsing logic (can be behind a feature flag)

===========================================================
PACKAGING / STARTUP
===========================================================
- Provide systemd user service:
  ~/.config/systemd/user/rog-helperd.service
  Must support:
    systemctl --user enable --now rog-helperd
- Provide a .desktop file for rog-helper-ui
- Later: packaging targets (optional V1.1)
  - Arch PKGBUILD
  - Debian/Ubuntu .deb
  - Flatpak (only after DBus permission model is understood)

===========================================================
ACCEPTANCE CRITERIA (DEFINITION OF DONE)
===========================================================
A build is considered “V1 DONE” when:
- App runs without root.
- Tray menu works and shows live status.
- Dashboard shows telemetry (best-effort).
- Profiles switching works on supported hardware (asusd present).
- Battery limit works (if supported).
- Lighting works (if supported).
- GPU mode switching works (if supported) with safe UX around reboot/logout/busy conditions.
- Fan curves are either:
    (a) fully supported with validation + presets + apply/revert, OR
    (b) clearly shown as not supported with read-only fan RPM display.
- Auto mode works with debouncing and manual override.
- Diagnostics page clearly explains missing services / unsupported features.
- Errors are human-friendly and exportable.

===========================================================
YOUR FIRST OUTPUTS (WHAT YOU MUST PRODUCE IMMEDIATELY)
===========================================================
1) A scaffolded repo with the workspace layout above.
2) rog-cli that can:
   - print service presence
   - print DBus endpoints
   - print detected sensors
   - print DeviceCaps summary
3) A minimal daemon exposing session DBus methods:
   - GetCaps, GetState, GetTelemetry
4) A minimal UI showing Dashboard + Diagnostics + Tray

Then proceed milestone by milestone.

FINAL INSTRUCTION
Do not guess DBus interfaces. Ask for introspection outputs and implement against them. Build safely, validate aggressively, degrade gracefully, and document everything.
