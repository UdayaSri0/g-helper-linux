# Codex Prompt: Add Safe Fan Control Page, Individual Fan Controls, Sync Mode, Manual Boost, and Fan Curve Settings to g-helper-linux

Repository: https://github.com/UdayaSri0/g-helper-linux

You are working on an existing Rust/GTK/libadwaita Linux desktop application for ASUS ROG laptop control. Implement a production-quality, safety-first fan control feature. This must integrate with the existing architecture instead of bypassing it.

## Core goal

Add a complete **Fans** section to the application with:

1. Fan RPM monitoring for 0..N fans.
2. A new **Fans** page in the GTK/libadwaita UI.
3. Individual fan cards for every detected fan.
4. Optional **Sync fans** mode so all controllable fans can share the same manual percentage or curve.
5. Manual fan speed control where supported.
6. Custom fan curve editing where supported.
7. Temporary **Full Speed Boost** / **Max Fan** mode.
8. Safe fallback to BIOS/Auto mode.
9. Capability-aware UI and diagnostics.
10. Updated docs, tests, and release notes.

This feature must never assume that every ASUS laptop supports fan control. It must detect support, expose read-only telemetry when writes are not possible, and clearly explain missing/unsupported/permission-denied states.

---

## Important existing architecture constraints

Respect the current project architecture:

- `rog-ui` must remain unprivileged.
- `rog-ui` must not write directly to `/sys`, system D-Bus, procfs, or hardware interfaces.
- Hardware-affecting actions must go through `rog-helperd`.
- `rog-helperd` must route fan actions through provider modules.
- Provider-level code belongs in `crates/rog-providers`.
- Shared fan types, validation, and errors belong in `crates/rog-core`.
- Diagnostics and CLI probing belong in `crates/rog-cli`.
- Existing DBus payload style uses extensible `a{sv}` dictionaries. Follow the existing pattern unless the repository has already moved to a typed shared schema.
- Keep the UI capability-driven. Do not enable fan controls unless the daemon reports that the action is supported.

Before coding, inspect these files and update the correct ones:

- `crates/rog-core/src/model.rs`
- `crates/rog-core/src/error.rs`
- `crates/rog-providers/src/traits.rs`
- `crates/rog-providers/src/hwmon.rs`
- `crates/rog-providers/src/asusd.rs`
- `crates/rog-providers/src/lib.rs`
- `crates/rog-daemon/src/main.rs`
- `crates/rog-ui/src/main.rs`
- `crates/rog-cli/src/main.rs`
- `docs/FEATURE_MATRIX.md`
- `docs/PROVIDER_MATRIX.md`
- `docs/DBUS_API.md`
- `docs/GUI_SPEC.md`
- `docs/UI_PAGES.md`
- `docs/TROUBLESHOOTING.md`
- `docs/HARDWARE_SUPPORT.md`
- `docs/ROADMAP.md`
- `README.md`

---

## Linux fan-control reality

Implement this feature based on actual Linux fan-control behaviour:

- `fan*_input` usually reports RPM and is normally read-only.
- `pwm*` usually represents duty/percentage-style control, not exact RPM.
- `fan*_target` represents desired RPM only when the hardware/driver supports closed-loop fan control.
- Many laptops expose RPM reading but not writable fan control.
- Some ASUS laptops expose fan curves through `asusd` / `asusctl`-compatible interfaces, but this is hardware-dependent.
- Firmware/BIOS may override software fan settings.
- Therefore, the app must prefer safe percentage/curve controls and only expose exact RPM targeting if a backend explicitly confirms support.

Do not present exact RPM sliders as universally available. If exact RPM target files or backend APIs are not available, hide RPM target controls and show percentage-based controls only.

---

## Feature design

### 1. New shared fan domain model in `rog-core`

Add or extend fan-related domain types. If similar types already exist, extend them rather than duplicating.

Suggested model concepts:

```rust
pub struct FanInfo {
    pub id: String,
    pub index: u32,
    pub label: String,
    pub current_rpm: Option<u32>,
    pub min_rpm: Option<u32>,
    pub max_rpm: Option<u32>,
    pub current_percent: Option<u8>,
    pub controllable: bool,
    pub supports_manual_percent: bool,
    pub supports_manual_rpm_target: bool,
    pub supports_curve: bool,
    pub supports_auto: bool,
    pub backend: String,
    pub endpoints: Vec<String>,
    pub notes: Vec<String>,
    pub warnings: Vec<String>,
}

pub enum FanControlMode {
    Auto,
    ManualPercent,
    ManualRpmTarget,
    Curve,
    FullSpeedBoost,
    Unsupported,
    ReadOnly,
}

pub struct FanCurvePoint {
    pub temp_c: u8,
    pub speed_percent: u8,
}

pub struct FanCurve {
    pub fan_id: String,
    pub profile: Option<String>,
    pub points: Vec<FanCurvePoint>,
}

pub struct FanControlRequest {
    pub fan_id: String,
    pub mode: FanControlMode,
    pub speed_percent: Option<u8>,
    pub rpm_target: Option<u32>,
    pub curve: Option<FanCurve>,
    pub duration_seconds: Option<u64>,
}
```

Keep the final shape consistent with the existing codebase style. These examples are guidance, not mandatory exact code.

### 2. Validation rules

Add strict validation in `rog-core`, not only in the UI.

Minimum safety validation:

- Fan speed percentage must be `0..=100`.
- Manual RPM control must only be accepted if the backend reports `supports_manual_rpm_target == true`.
- Fan curves must have ordered temperature points.
- Fan curves must have ordered or non-decreasing speed percentages unless the backend specifically allows otherwise.
- Fan curve temperature range must be conservative and safe. Suggested default: `30..=100°C`.
- Curves must force high fan speed at high temperatures. Suggested minimum:
  - `>= 85°C` must be at least `70%`
  - `>= 90°C` must be at least `90%`
  - `>= 95°C` must be `100%`
- Reject obviously dangerous curves, for example `90°C -> 20%`.
- Reject empty curves.
- Reject fan IDs that do not exist in the current daemon fan inventory.
- For manual mode, never allow permanent hidden manual override without a visible UI state and a clear revert option.

Use clear `RogError` variants/messages such as:

- `NotSupported`
- `PermissionDenied`
- `InvalidInput`
- `DependencyMissing`
- `TemporarilyUnavailable`

---

## Provider implementation

### 3. Extend the provider trait

In `crates/rog-providers/src/traits.rs`, define or extend a `FanProvider` abstraction.

Suggested methods:

```rust
async fn fan_caps(&self) -> Result<FanCaps, RogError>;
async fn list_fans(&self) -> Result<Vec<FanInfo>, RogError>;
async fn get_fan_state(&self) -> Result<FanState, RogError>;
async fn set_fan_auto(&self, fan_id: Option<&str>) -> Result<(), RogError>;
async fn set_fan_manual_percent(&self, fan_id: &str, percent: u8) -> Result<(), RogError>;
async fn set_fan_rpm_target(&self, fan_id: &str, rpm: u32) -> Result<(), RogError>;
async fn set_fan_curve(&self, fan_id: &str, curve: FanCurve) -> Result<(), RogError>;
async fn restore_fan_defaults(&self) -> Result<(), RogError>;
```

Use synchronous methods if the existing provider style is synchronous. Match the repository's style.

### 4. Extend `hwmon` fan discovery

Improve `crates/rog-providers/src/hwmon.rs` so it discovers:

- `fan*_input`
- `fan*_label`
- `fan*_min`
- `fan*_max`
- `fan*_target`
- `pwm*`
- `pwm*_enable`
- `pwm*_mode`
- `pwm*_auto_point*`
- `temp*_input`
- `temp*_label`

Map fans dynamically as `0..N`, not hardcoded CPU/GPU only.

Expose read-only fan telemetry even when control is not writable.

For control support:

- Manual percent control is available only if matching `pwmN` and `pwmN_enable` are writable.
- Manual RPM target is available only if `fanN_target` exists and is writable.
- Curve support is available only if the required auto-point files exist and are writable.
- Treat permission errors as read-only, not fatal.
- Add endpoint paths to diagnostics.

Important: do not guess fan-to-CPU/GPU mapping if labels are missing. Use friendly fallback labels:

- `Fan 1`
- `Fan 2`
- `Fan 3`
- `Fan 4`

If labels suggest CPU/GPU, show:

- `CPU Fan`
- `GPU Fan`
- `System Fan`
- `Aux Fan`

### 5. ASUS/asusd fan curve provider

Inspect the currently installed/expected `asusd` interfaces using existing D-Bus probing helpers. Do not hardcode unverified ASUS D-Bus object paths or method names unless they are already used elsewhere in the project and confirmed.

Implement fan-curve support through `asusd` only if the API is discoverable and testable.

Requirements:

- Probe whether `asusd` supports fan curves.
- Support profile-linked curves where available:
  - Quiet / Silent
  - Balanced
  - Performance / Turbo
- Report the backend as `asusd` when using ASUS fan-curve control.
- If `asusd` is present but fan curves are not exposed, report fan curve control as unsupported, not broken.
- If `asusd` rejects a write, surface a useful warning.

Do not implement shelling out to `asusctl` as the primary backend unless there is no better option and it is clearly documented as a fallback. Prefer direct D-Bus provider integration.

### 6. Backend priority

Use this backend priority:

1. `asusd` fan curve backend, if available and verified.
2. Generic `hwmon` PWM/manual backend, if writable and safe.
3. Read-only `hwmon` telemetry.
4. Unsupported/read-only state with clear diagnostics.

Never write to a sysfs path just because it exists. Confirm it is the correct fan-control path and is writable.

---

## Daemon and DBus API

### 7. Add daemon fan state

Extend `rog-helperd` state with:

- fan capability summary
- dynamic fan list
- current fan mode
- sync mode enabled/disabled
- last fan action result
- active boost state, if any
- active curve summary, if available
- warnings/notes about fan backend support

Suggested capability keys:

- `has_fan_reading`
- `has_fan_curves`
- `has_fan_manual_percent`
- `has_fan_manual_rpm_target`
- `has_individual_fan_control`
- `has_fan_sync_control`
- `has_fan_boost`
- `fan_count`
- `fan_backend`

Preserve existing `has_fan_reading` and `has_fan_curves` semantics, but make them accurate.

### 8. Add DBus methods

Add methods consistent with the existing `a{sv}` style:

- `GetFanCaps() -> a{sv}`
- `GetFanState() -> a{sv}`
- `GetFanCurves() -> a{sv}` or `as`/nested maps based on current style
- `SetFanAuto(fan_id: s)` where empty string means all fans if supported
- `SetFanManualPercent(fan_id: s, percent: t/u64)`
- `SetFanRpmTarget(fan_id: s, rpm: t/u64)` only if supported
- `SetFanCurve(fan_id: s, curve: a{sv})`
- `SetFanSync(enabled: b)`
- `SetFanBoost(fan_id: s, percent: t/u64, duration_seconds: t/u64)`
- `ResetFansToAuto()`

Also include fan fields inside `GetState` so the UI can refresh normally.

### 9. Daemon safety behaviour

Implement safety behaviour in daemon, not only UI:

- On daemon startup, do not automatically take manual control of fans.
- On manual apply, store previous mode if possible.
- On daemon normal shutdown, attempt to restore Auto/BIOS mode.
- On full-speed boost timeout, restore previous mode or Auto.
- If temperature telemetry disappears while manual control is active, restore Auto.
- If CPU or GPU temperature crosses a critical threshold, force safe/full fan or restore firmware/Auto depending on backend semantics.
- If a fan write fails halfway through multiple fans, attempt rollback and report partial failure.
- If sync mode is enabled, apply the same setting to all controllable fans only; leave read-only fans unchanged and report them clearly.
- Never silently continue in unsafe manual mode after backend errors.

### 10. Manual control modes

Support these modes in daemon and UI:

#### Auto / BIOS Default

Return control to firmware/backend automatic mode.

#### Manual percentage

Set one fan or all synced fans to a percentage.

Use percentages in UI. Convert to PWM internally only in provider code.

Example conversion:

- `0% -> 0`
- `50% -> 128`
- `100% -> 255`

But respect backend limits if they differ.

#### RPM target

Only show and enable this if the backend explicitly reports `supports_manual_rpm_target`.

Do not show RPM target as the primary UI control.

#### Full Speed Boost

Temporary max cooling mode:

- 5 minutes
- 10 minutes
- 15 minutes
- Custom duration, optional
- Stop Boost button

Default boost should be all controllable fans synced to 100%.

Show countdown in the UI.

#### Custom curve

A curve editor using temperature-to-speed points.

Support individual curves per fan where backend supports it.

Support synced curve mode where the same curve applies to all controllable fans.

---

## UI implementation

### 11. Add a new Fans page

Add a new **Fans** page to the main navigation/ViewStack.

Page icon suggestions:

- `fan-symbolic` if available
- fallback: `preferences-system-symbolic`
- fallback: `utilities-system-monitor-symbolic`

The page should be capability-driven and must remain useful even when control is unsupported.

### 12. Fans page layout

Suggested layout:

#### Header/status section

Show:

- fan backend: `asusd`, `hwmon`, `read-only`, `unsupported`
- number of detected fans
- current mode: Auto / Manual / Curve / Boost
- warning banner if fan control is experimental, unsupported, or read-only
- “Return to Auto” button
- “Copy fan diagnostics” button

#### Fan cards

For each detected fan, show:

- fan label
- current RPM
- current percent if available
- backend endpoint summary
- controllable/read-only badge
- last update time or status
- warning if mapping is uncertain

Each card should have controls only when supported:

- manual percentage slider
- optional RPM target field if supported
- Auto button for that fan
- Boost button for that fan
- Curve editor button/section for that fan

#### Sync controls

Add a **Sync fans** toggle:

- When enabled, one master slider controls all controllable fans.
- When disabled, each fan card is independently adjustable.
- Read-only fans remain visible but disabled.
- If the laptop has only one fan, hide or disable sync toggle with explanation.

#### Manual slider

Use percentage as the main slider:

- `0%` to `100%`
- default step: `5%`
- show approximate PWM value only in advanced details
- show current RPM beside it, but do not promise exact RPM

Before applying manual mode for the first time, show a confirmation dialog:

Title: `Manual fan control`
Message: `Manual fan control may increase noise, reduce cooling if configured incorrectly, or be overridden by firmware. Use Auto mode if you are unsure.`
Buttons:
- Cancel
- Enable Manual Control

Store acknowledgement in the settings only if the project already has a safe preferences mechanism. Otherwise ask each session.

#### Full speed boost section

Show:

- `Boost all fans to 100% for 5 min`
- `Boost all fans to 100% for 10 min`
- `Boost all fans to 100% for 15 min`
- `Stop boost`

The boost must be time-limited unless the user explicitly chooses an advanced custom option.

#### Fan curve editor

Provide a simple curve editor:

- 8-point default curve if backend expects 8 points.
- Otherwise support backend-reported number of points.
- Editable rows:
  - temperature °C
  - speed %
- Buttons:
  - Load safe default
  - Load quiet default
  - Load performance default
  - Apply curve
  - Reset to BIOS/Auto
- Show validation errors inline before applying.
- Do not allow dangerous curves.

Suggested default safe curve:

- 35°C -> 15%
- 45°C -> 25%
- 55°C -> 35%
- 65°C -> 50%
- 75°C -> 70%
- 85°C -> 90%
- 90°C -> 100%
- 95°C -> 100%

For laptops/backends that require exact 8 points, always send exactly 8 points.

### 13. UI state and error handling

The UI must:

- Disable unsupported controls.
- Show read-only messages instead of failing silently.
- Use existing toast/status pattern for successful and failed fan actions.
- Show no raw stack traces.
- Keep diagnostics copyable.
- Refresh fan state through existing polling model.
- Continue working if no fans are detected.
- Continue working if fan telemetry exists but control is unsupported.

---

## Persistent settings

If the project already has a settings/persistence mechanism, extend it safely.

Persist only safe user preferences:

- fan warning acknowledgement
- sync mode preference
- preferred view mode
- saved custom curves, but do not auto-apply them on startup unless the user explicitly enables that later

Do not automatically reapply manual fan speeds on startup in this change. That is dangerous.

If saved curves are implemented:

- store under XDG config path consistent with the project
- validate before saving and before applying
- include schema version
- include model/backend compatibility metadata
- do not apply a curve saved from another backend or machine without confirmation

---

## CLI diagnostics

Add fan-focused CLI commands or extend existing diagnostics:

Suggested commands:

```bash
cargo run -p rog-cli -- fans
cargo run -p rog-cli -- fan-caps
cargo run -p rog-cli -- sensors
cargo run -p rog-cli -- caps
```

Output should include:

- detected fans
- RPM readings
- labels
- backend names
- control support
- writable endpoints
- read-only endpoints
- fan curve support
- sync support
- warnings
- exact reason if unsupported

Do not require root for diagnostics. If root would reveal more data, say so clearly without failing.

---

## Documentation updates

Update documentation in the same PR/change.

Required docs:

### `docs/FEATURE_MATRIX.md`

Update fan rows:

- Fan RPM telemetry: implemented
- Fan manual percent control: implemented/partial depending on backend
- Fan RPM target: optional/backend-dependent
- Fan curves: implemented/partial/backend-dependent
- Sync fan control: implemented if more than one controllable fan
- Boost mode: implemented

### `docs/PROVIDER_MATRIX.md`

Document:

- `hwmon` fan telemetry and optional PWM control
- `asusd` fan curve backend if implemented
- permission limits
- backend priority
- exact unsupported states

### `docs/DBUS_API.md`

Document all new fan methods and payload keys.

### `docs/GUI_SPEC.md` and `docs/UI_PAGES.md`

Add the Fans page and all controls.

### `docs/TROUBLESHOOTING.md`

Add troubleshooting sections:

- Fans visible but controls disabled
- Fan RPM appears but manual control unavailable
- Permission denied writing PWM/sysfs
- `asusd` present but fan curves unsupported
- Firmware/BIOS overriding curves
- Fan labels unknown
- Boost mode failed or restored Auto

### `docs/HARDWARE_SUPPORT.md`

Add fan-control validation fields:

- fan count detected
- labels detected
- RPM reading works
- manual percent works
- RPM target works
- fan curve works
- sync mode works
- boost mode works
- restore Auto works
- backend used
- notes/warnings

### `README.md`

Update feature list carefully:

Good wording:
`Fan monitoring and safe fan controls for supported hardware`

Avoid:
`Full fan control for all ASUS laptops`

---

## Testing requirements

Add unit tests where possible:

### Core validation tests

Test:

- valid curve accepted
- empty curve rejected
- unordered temperature rejected
- unsafe high-temperature/low-speed curve rejected
- out-of-range percentages rejected
- RPM target rejected when unsupported
- unknown fan ID rejected

### Provider tests

Where direct hardware tests are not possible:

- use temp directories or mock provider state
- simulate readable fan inputs
- simulate writable PWM paths
- simulate read-only paths
- verify permission-denied behaviour
- verify endpoint reporting

### Daemon tests

Test:

- capability maps include fan fields
- invalid fan requests return `InvalidArgs`
- unsupported requests return `NotSupported`
- boost timeout restores previous mode/Auto
- sync mode applies only to controllable fans
- partial failures are reported

### UI compile/behaviour checks

At minimum:

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Do not leave unused warnings or clippy failures.

---

## Safety acceptance criteria

This feature is acceptable only if:

- UI never writes to hardware directly.
- Unsupported fan control degrades to read-only telemetry.
- Manual fan control requires explicit confirmation.
- Full-speed boost is time-limited.
- Auto/BIOS restore is always available.
- Daemon attempts to restore Auto on shutdown.
- Dangerous fan curves are rejected.
- Exact RPM controls are only shown when explicitly supported.
- Sync mode does not hide individual fan status.
- Diagnostics clearly explain what is supported and what is not.
- Documentation clearly states hardware support is model/backend-dependent.

---

## Implementation plan

Work in this order:

1. Inspect current fan-related domain model and provider traits.
2. Add/extend fan model and validation in `rog-core`.
3. Improve `hwmon` fan discovery for dynamic 0..N fan telemetry and optional control capability detection.
4. Add ASUS/asusd fan curve probing only if the interface is discoverable and safe.
5. Add daemon fan state and DBus methods.
6. Add CLI fan diagnostics.
7. Add Fans page UI with read-only fan cards first.
8. Add manual percentage controls.
9. Add sync mode.
10. Add temporary full-speed boost.
11. Add fan curve editor.
12. Add safety restore logic.
13. Add tests.
14. Update docs.
15. Run full validation commands.
16. Summarise what works, what is partial, and what still depends on hardware support.

---

## Final output expected from Codex

After implementation, provide:

1. Summary of changed files.
2. Explanation of detected fan-control backend strategy.
3. New DBus methods and payload keys.
4. Safety behaviour summary.
5. Testing performed and results.
6. Known limitations.
7. Manual test checklist for ASUS ROG laptops.

Manual test checklist must include:

```bash
cargo run -p rog-daemon
cargo run -p rog-ui
cargo run -p rog-cli -- fans
cargo run -p rog-cli -- fan-caps
cargo run -p rog-cli -- sensors
cargo run -p rog-cli -- caps
```

Also include system inspection commands:

```bash
busctl --system list | grep -Ei "asus|rog|supergfx|power|upower"
find /sys/class/hwmon -maxdepth 3 -type f \( -name "fan*_input" -o -name "fan*_label" -o -name "pwm*" -o -name "fan*_target" \) -print
```

Do not claim universal support. Phrase the final feature as:

`Fan monitoring and safe fan controls for supported ASUS/Linux hardware.`

