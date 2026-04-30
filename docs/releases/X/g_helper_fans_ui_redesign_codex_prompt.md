# Codex Prompt: Redesign Fans Page UI with RPM Gauges, Animated Fan Rotors, Individual Fan Cards, and Safe Control UX

Repository: https://github.com/UdayaSri0/g-helper-linux

You are improving the existing `g-helper-linux` / `rog-helper` Fans page after the first fan-control implementation.

The current Fans page is functional but visually too plain and text-heavy. It already detects fan telemetry correctly on the target laptop, but the UI needs to become much more polished, modern, and useful.

The latest diagnostics from the user’s laptop show:

```text
Fan Control Diagnostics
=======================
backend: read-only
fan_count: 4
mode: Auto / BIOS Default
sync_enabled: false
manual_percent: false
manual_rpm_target: false
curves: false
boost: false

Fans
----
Fan 1 [hwmon1:fan1]: 2880 rpm
  percent: (not reported)
  backend: hwmon-read-only; controllable: false; manual_percent: false; rpm_target: false; curve: false; auto: false
  endpoints: fan_input:/sys/class/hwmon/hwmon1/fan1_input
  note: Telemetry only; no writable fan-control endpoint was confirmed.
  warning: Fan mapping is unlabeled by hwmon; CPU/GPU assignment is uncertain.

CPU Fan [hwmon8:fan1]: 2800 rpm
  percent: (not reported)
  backend: hwmon-read-only; controllable: false; manual_percent: false; rpm_target: false; curve: false; auto: false
  endpoints: fan_input:/sys/class/hwmon/hwmon8/fan1_input, pwm_enable:/sys/class/hwmon/hwmon8/pwm1_enable
  note: Telemetry only; no writable fan-control endpoint was confirmed.

GPU Fan [hwmon8:fan2]: 3000 rpm
  percent: (not reported)
  backend: hwmon-read-only; controllable: false; manual_percent: false; rpm_target: false; curve: false; auto: false
  endpoints: fan_input:/sys/class/hwmon/hwmon8/fan2_input, pwm_enable:/sys/class/hwmon/hwmon8/pwm2_enable
  note: Telemetry only; no writable fan-control endpoint was confirmed.

Mid Fan [hwmon8:fan3]: 3900 rpm
  percent: (not reported)
  backend: hwmon-read-only; controllable: false; manual_percent: false; rpm_target: false; curve: false; auto: false
  endpoints: fan_input:/sys/class/hwmon/hwmon8/fan3_input, pwm_enable:/sys/class/hwmon/hwmon8/pwm3_enable
  note: Telemetry only; no writable fan-control endpoint was confirmed.

Warnings
--------
- Fan mapping is unlabeled by hwmon; CPU/GPU assignment is uncertain.
```

The improved UI must handle this exact real-world state beautifully:

- 4 fans detected
- read-only backend
- RPM telemetry available
- manual controls disabled
- percentage not reported
- fan mapping partly uncertain
- CPU/GPU/Mid labels available for some fans
- no curves/boost/manual control available yet

The UI must look polished even when controls are unavailable.

---

## Design direction

Create an original UI inspired by gaming laptop fan dashboards, but do not copy any proprietary ASUS/Armoury Crate/G-Helper design exactly.

The visual direction should be:

- dark, modern, ROG/gaming-inspired
- neon accent glow, but still clean and readable
- GTK/libadwaita-native, not a webview
- professional enough for open-source Linux software
- usable in both dark and light mode
- responsive for smaller laptop screens
- accessible, not just flashy

Use the attached reference image only as inspiration for:

- circular temperature/RPM gauges
- animated fan visuals
- left/right fan blocks
- central control mode section
- strong visual hierarchy

Do not reproduce the reference layout pixel-for-pixel, do not use copyrighted assets, and do not use ASUS branding unless it already exists legally in the project.

---

## Technology constraints

Prefer the existing stack:

- Rust
- GTK4
- libadwaita
- CSS resources
- Cairo drawing via `gtk::DrawingArea`
- existing polling/cache state model
- existing DBus calls through `rog-helperd`

You may add additional source files/modules if it improves maintainability, for example:

```text
crates/rog-ui/src/fans_page.rs
crates/rog-ui/src/fan_widgets.rs
crates/rog-ui/src/fan_theme.rs
crates/rog-ui/src/fan_curve_editor.rs
crates/rog-ui/resources/fans.css
```

If the current crate does not use GTK resources yet, integrate the CSS in the simplest maintainable way that matches the existing project. Do not introduce a web frontend, Electron, Tauri, WebKit, or a separate JavaScript UI.

Allowed UI implementation approaches:

- custom GTK widgets using `DrawingArea`
- CSS classes for cards/glow/badges
- libadwaita `Clamp`, `PreferencesGroup`, `ActionRow`, `ExpanderRow`, `ToastOverlay`
- `glib::timeout_add_local` for animation ticks
- lightweight math for gauge arcs and fan rotation

Do not add heavy dependencies unless absolutely necessary.

---

## Main UI objective

Replace the current plain Fans page with a richer dashboard that has:

1. A top visual fan dashboard area.
2. Animated fan rotors that spin based on live RPM.
3. Circular RPM/temperature gauges.
4. Individual fan cards for all detected fans.
5. Clear read-only/controllable status badges.
6. Disabled controls that still explain why they are disabled.
7. A collapsible diagnostics section instead of raw diagnostics always occupying the main visual area.
8. A safe control area that becomes active only when backend support exists.

---

## Required UI layout

### 1. Top header

Add a polished header at the top of the Fans page:

- Title: `Fan Control`
- Subtitle: `Monitor cooling performance and use safe fan controls when supported.`
- Backend badge:
  - `read-only`
  - `hwmon`
  - `asusd`
  - `unsupported`
- Fan count badge:
  - `4 fans detected`
- Mode badge:
  - `Auto / BIOS Default`
- Warning badge if fan mapping is uncertain:
  - `Mapping uncertain`

The header should not look like plain text. Use a card or hero header with subtle accent styling.

### 2. Hero dashboard

Create a dashboard section using custom drawing widgets.

Suggested layout:

```text
+---------------------------------------------------------------+
| Fan Control                                                   |
| backend: read-only | 4 fans detected | Auto / BIOS Default    |
+---------------------------------------------------------------+
|                                                               |
|  [GPU/CPU temp gauge]       [Mode controls]      [CPU temp gauge]
|                                                               |
|  [Fan 1 rotor] [CPU rotor] [GPU rotor] [Mid rotor]             |
|  RPM values shown under each rotor                            |
|                                                               |
+---------------------------------------------------------------+
```

This does not need to be identical to the reference image. It should be native GTK and responsive.

When CPU/GPU temperature is available:

- show circular temp gauges:
  - CPU temperature
  - GPU temperature
- show large temperature number in °C
- show small label: CPU / GPU
- if temperature is unavailable, show `-- °C`

When fan RPM is available:

- show animated fan rotor for each fan
- show RPM as large text:
  - `2880 RPM`
  - `2800 RPM`
  - `3000 RPM`
  - `3900 RPM`
- show label:
  - `Fan 1`
  - `CPU Fan`
  - `GPU Fan`
  - `Mid Fan`

If more than 4 fans exist:

- show first 4 in the hero
- show all fans in the individual fan grid below

If fewer than 4 fans exist:

- gracefully adapt the layout

### 3. Animated fan rotor widget

Implement a reusable `FanRotorWidget` or drawing function.

Requirements:

- Draw a circular fan ring.
- Draw 3 or 5 fan blades.
- Rotate blades according to live RPM.
- Show status ring around the fan:
  - normal telemetry: accent ring
  - read-only: muted/blue-grey ring
  - controllable: brighter accent ring
  - warning: amber ring
  - error/unavailable: red/muted ring
- Show RPM text either inside or below the widget.
- Use smooth animation but cap visual spin speed so it does not become distracting.
- The visual spin speed must be derived from RPM but scaled for human readability.

Suggested animation mapping:

```text
rpm = 0        -> no rotation
1..1000 rpm   -> slow rotation
1000..3000    -> medium rotation
3000..5000    -> fast rotation
5000+         -> capped fast rotation
```

Example formula:

```rust
let rpm = fan.current_rpm.unwrap_or(0) as f64;
let visual_rps = (rpm / 1500.0).clamp(0.0, 4.0);
angle += visual_rps * 2.0 * PI * delta_seconds;
```

Important:

- Do not attempt to render true physical RPM; that would be visually unusable.
- Keep CPU use low.
- Stop or slow animation when the page is not visible if possible.
- Respect GTK settings for reduced motion if accessible. If the setting is unavailable, add a local boolean in code to easily disable animation later.

### 4. Circular gauge widget

Implement a reusable circular gauge widget or drawing helper for:

- temperature gauges
- optional RPM gauges

Gauge requirements:

- semi-circular or circular arc
- tick marks from 0 to 100 for temperature
- central large value text
- label text
- subtle glow/accent
- safe/warning/hot visual states:
  - below 70°C normal
  - 70–85°C warm
  - 85°C+ hot
- support unavailable state with `--`

Do not use hardcoded CPU/GPU assumptions if telemetry is missing.

### 5. Individual fan cards

Below the hero, show a responsive grid/list of individual fan cards.

Each card must include:

- fan label
- fan ID/backend endpoint short name, for example:
  - `hwmon8:fan1`
- current RPM
- status badge:
  - `Read-only`
  - `Controllable`
  - `Telemetry only`
  - `Mapping uncertain`
- compact animated fan rotor
- backend text:
  - `hwmon-read-only`
- endpoint summary, collapsed by default
- note/warning area if relevant

For the current user’s state, the cards should show:

- Fan 1: 2880 RPM, read-only, mapping uncertain
- CPU Fan: 2800 RPM, read-only
- GPU Fan: 3000 RPM, read-only
- Mid Fan: 3900 RPM, read-only

Do not hide read-only fans. Read-only telemetry is still useful.

### 6. Controls panel

Keep controls visible but capability-aware.

For the current backend:

- Manual speed slider must be disabled.
- Boost buttons must be disabled.
- Curve editor must be disabled.
- Sync toggle must be disabled or marked unavailable.
- The UI must clearly say:
  - `Fan RPM telemetry is available, but no writable fan-control endpoint was confirmed.`
  - `Controls will be enabled automatically when a supported fan-control backend is detected.`

When controls are supported in the future:

- Enable manual percentage slider.
- Enable per-fan control.
- Enable sync mode when more than one fan is controllable.
- Enable boost buttons.
- Enable curve editor.

The controls panel should include:

- Mode selector:
  - Auto / BIOS Default
  - Manual
  - Curve
  - Full Speed Boost
- Sync fans toggle
- Master fan speed slider
- Boost buttons:
  - 5 min
  - 10 min
  - 15 min
  - Stop
- Return to Auto button
- Copy fan diagnostics button

### 7. Fan curve preview

Even if curves are unsupported, add a disabled visual curve preview section.

The section should show:

- default safe curve preview
- status text:
  - `Fan curve editing is not available on this backend.`
- disabled Apply button
- disabled Reset button

When curve support is available later, this same component should become editable.

Use a simple custom `DrawingArea` or GTK layout. No heavy charting library.

Default preview curve:

```text
35°C -> 15%
45°C -> 25%
55°C -> 35%
65°C -> 50%
75°C -> 70%
85°C -> 90%
90°C -> 100%
95°C -> 100%
```

### 8. Diagnostics section

Move raw diagnostics into a collapsed or secondary section.

Requirements:

- Keep `Copy fan diagnostics`
- Add `Show diagnostics` / `Hide diagnostics`
- Diagnostics must include the exact text currently shown, but not dominate the page.
- Warnings should be shown as readable warning banners/cards above diagnostics.
- No raw stack traces in the main UI.

---

## Styling requirements

Add a refined CSS style for the Fans page.

Suggested CSS classes:

```css
.fans-page
.fans-hero
.fans-header-card
.fans-status-pill
.fans-status-pill-read-only
.fans-status-pill-controllable
.fans-warning-banner
.fan-visual-card
.fan-card
.fan-card-read-only
.fan-card-controllable
.fan-rpm-large
.fan-rpm-label
.fan-control-panel
.fan-curve-card
.fan-diagnostics-box
.neon-blue
.neon-magenta
.neon-muted
```

Design requirements:

- Use subtle gradient backgrounds, not aggressive full-screen neon.
- Use rounded cards consistent with libadwaita.
- Use enough contrast for text.
- Support small window widths.
- Avoid fixed pixel-only layouts that break on different screens.
- Respect system theme as much as possible.
- Do not make the app look like a game launcher at the cost of usability.

The UI can have a gaming feel, but it must still feel like a Linux system utility.

---

## Responsiveness

The layout must adapt:

### Wide window

- Hero dashboard can show CPU/GPU gauges and multiple fans in one row.
- Controls can sit beside fan visuals.

### Medium window

- Hero sections stack in two rows.
- Fan cards use two columns.

### Narrow window

- One column layout.
- Gauges and fan cards stack vertically.
- No horizontal clipping.

Avoid requiring users to maximise the app.

---

## Data handling rules

Use real daemon data only.

Do not invent fan percentages if the backend does not report them.

For the current diagnostics:

- show RPM
- show percent as `Not reported`
- show manual controls disabled
- do not estimate or pretend control exists

If you add visual normalisation for gauges:

- label it as visual scale, not actual percentage
- do not expose it as a real fan percentage

Good wording:

```text
RPM telemetry available
Fan speed percentage not reported by backend
Control endpoint not writable
```

Bad wording:

```text
Fan is at 60%
```

unless the backend actually reports percentage or PWM mapping.

---

## Animation performance and safety

The animation must be lightweight.

Requirements:

- One shared animation timer for the Fans page where possible.
- Do not create one independent high-frequency timer per fan card if avoidable.
- 30 FPS is enough; 60 FPS is optional but unnecessary.
- Stop animation when there are no RPM values.
- Cap visual speed.
- Do not block GTK main loop.
- Do not perform DBus calls inside draw callbacks.
- Drawing callbacks must only render current cached state.

Suggested:

```rust
glib::timeout_add_local(Duration::from_millis(33), clone!(... => @default-return ControlFlow::Break, move || {
    update_animation_angles();
    queue_draw_for_visible_fan_widgets();
    ControlFlow::Continue
}));
```

If the project currently uses a different GTK/glib pattern, follow the existing pattern.

---

## Architecture and maintainability

Do not keep growing `crates/rog-ui/src/main.rs` endlessly if it is already too large.

Preferred refactor:

- Extract Fans page creation into `fans_page.rs`
- Extract custom drawing widgets/helpers into `fan_widgets.rs`
- Keep DBus/state decoding in the existing UI state layer unless there is already a better pattern
- Avoid duplicating fan state parsing in several places

Suggested functions:

```rust
fn build_fans_page(shared_state: SharedUiState, toast_overlay: adw::ToastOverlay) -> gtk::Widget
fn build_fans_header(...) -> gtk::Widget
fn build_fans_hero(...) -> gtk::Widget
fn build_fan_cards_grid(...) -> gtk::Widget
fn build_fan_controls_panel(...) -> gtk::Widget
fn build_fan_curve_preview(...) -> gtk::Widget
fn build_fan_diagnostics_section(...) -> gtk::Widget
```

Suggested custom draw helpers:

```rust
draw_fan_rotor(ctx, width, height, rpm, angle, status)
draw_temperature_gauge(ctx, width, height, temp_c, label, accent)
draw_curve_preview(ctx, width, height, points, enabled)
```

Keep the code idiomatic Rust and GTK4.

---

## Accessibility

Add accessibility improvements:

- Use tooltips for disabled controls.
- Use text labels in addition to colour.
- Do not rely only on neon colours to communicate state.
- Provide static RPM text for users who disable animations.
- Use accessible names where GTK supports them.
- Make keyboard navigation reasonable.

For disabled controls, tooltips should explain the exact reason:

```text
Manual fan speed is unavailable because no writable fan-control endpoint was confirmed.
```

---

## Current-state acceptance criteria

Using the user’s current diagnostics, the UI must show:

- 4 detected fans
- backend: read-only
- mode: Auto / BIOS Default
- Fan 1: 2880 RPM
- CPU Fan: 2800 RPM
- GPU Fan: 3000 RPM
- Mid Fan: 3900 RPM
- animated rotors for each fan
- controls disabled with clear explanation
- mapping warning for unlabeled fan
- diagnostics available but collapsed by default
- no false percentages
- no claim that manual control is available

---

## Future-supported-state acceptance criteria

On hardware/backends where fan control is supported, the same UI must be ready to show:

- enabled manual slider
- individual fan controls
- sync fans toggle
- boost buttons
- curve editor
- return to Auto
- status toasts after applying actions
- validation errors for unsafe curve settings

Do not hardcode the page to read-only just because the current test laptop is read-only.

---

## Files to inspect and likely update

Inspect first:

```text
crates/rog-ui/src/main.rs
crates/rog-ui/Cargo.toml
crates/rog-daemon/src/main.rs
crates/rog-core/src/model.rs
docs/GUI_SPEC.md
docs/UI_PAGES.md
docs/TROUBLESHOOTING.md
docs/FEATURE_MATRIX.md
```

Likely add/update:

```text
crates/rog-ui/src/fans_page.rs
crates/rog-ui/src/fan_widgets.rs
crates/rog-ui/src/fan_curve_editor.rs
crates/rog-ui/resources/fans.css
crates/rog-ui/src/main.rs
docs/GUI_SPEC.md
docs/UI_PAGES.md
docs/TROUBLESHOOTING.md
docs/FEATURE_MATRIX.md
README.md
```

If GTK resource infrastructure does not exist, keep CSS loading simple and documented.

---

## Testing requirements

Run:

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Also manually run:

```bash
cargo run -p rog-daemon
cargo run -p rog-ui
cargo run -p rog-cli -- fans
cargo run -p rog-cli -- fan-caps
cargo run -p rog-cli -- sensors
cargo run -p rog-cli -- caps
```

Manual UI test checklist:

- Fans page opens without crash.
- Top navigation icon still works.
- UI updates RPM values every polling cycle.
- Fan rotor animation changes with RPM.
- Animation does not freeze the UI.
- Read-only controls are disabled.
- Disabled controls have explanations.
- Diagnostics can be copied.
- Diagnostics can be expanded/collapsed.
- Window works at narrow and wide sizes.
- Dark theme is readable.
- Light theme is acceptable.
- No raw warnings/errors are printed into the main UI.
- `cargo clippy` passes with `-D warnings`.

---

## Do not do these things

Do not:

- copy the reference image exactly
- add ASUS copyrighted artwork
- add a webview/electron frontend
- fake fan percentages
- expose manual control if backend is read-only
- make DBus calls from drawing callbacks
- use one high-frequency timer per fan card
- hide read-only fans
- remove diagnostics
- break existing dashboard/CPU/GPU/battery/lighting pages
- break the current capability-driven design
- make the UI depend on root permissions
- claim universal fan-control support

---

## Final response expected from Codex

After implementing, provide:

1. Summary of the UI redesign.
2. List of changed files.
3. Explanation of the new fan rotor and gauge drawing system.
4. Explanation of how read-only state is represented.
5. Explanation of how controls behave when supported vs unsupported.
6. Testing commands run and results.
7. Known limitations.
8. Screenshots or instructions for taking screenshots, if possible.

Use this final wording for feature description:

```text
The Fans page now provides a polished RPM monitoring dashboard with animated fan rotors, circular gauges, individual fan cards, capability-aware controls, and collapsed diagnostics. Manual control remains disabled unless a writable backend is confirmed by the daemon.
```
