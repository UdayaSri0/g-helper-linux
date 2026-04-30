# Codex Prompt: Improve Fans Page Hero UI with Larger Side-by-Side CPU/GPU Gauges and Operating MHz Display

Repository: https://github.com/UdayaSri0/g-helper-linux

You are improving the existing Fans page UI in the Rust/GTK/libadwaita `g-helper-linux` / `rog-helper` project.

The current Fans page already has a much better visual design with:

- polished Fan Control header
- backend/fan count/mode/mapping status pills
- CPU/GPU temperature gauges
- animated fan rotor cards
- read-only control warning
- safe controls section
- diagnostics support

However, the current layout has an issue:

- CPU and GPU temperature gauges are too small.
- CPU and GPU gauges are stacked vertically on the left.
- They should be larger, more visually important, and placed side by side.
- The page should also show operating speed/frequency in MHz for CPU and GPU where telemetry is available.
- The hero section should feel more like a professional performance dashboard.

Use the current screenshot/result as the baseline. Improve the UI without breaking the existing fan telemetry, read-only state handling, or safety behaviour.

---

## Main goal

Redesign the Fans page hero/dashboard area so that:

1. CPU and GPU temperature gauges are larger.
2. CPU and GPU gauges appear side by side on wide and medium screens.
3. Each gauge also shows operating MHz/speed.
4. The cooling mode/control card is positioned cleanly below or beside the gauges depending on screen width.
5. Fan rotor cards remain below the gauges in a clean responsive grid.
6. The layout remains usable in narrow windows.
7. No unsupported data is faked.

---

## Current problem to fix

The current Fans page visually looks like this:

```text
[Header]

[GPU gauge small]
[CPU gauge small]       [Cooling Mode card]

[Fan 1] [CPU Fan] [GPU Fan] [Mid Fan]
```

Improve it to something closer to this:

```text
[Header]

+---------------------------------------------------------------+
| +---------------------------+ +-----------------------------+ |
| | GPU Gauge                 | | CPU Gauge                   | |
| | 48°C                      | | 64°C                        | |
| | GPU Temperature           | | CPU Temperature             | |
| | Core Clock: ---- MHz      | | Avg Clock: 4300 MHz         | |
| +---------------------------+ +-----------------------------+ |
|                                                               |
| +-----------------------------------------------------------+ |
| | Cooling Mode / Status / Controls summary                  | |
| +-----------------------------------------------------------+ |
|                                                               |
| [Fan 1] [CPU Fan] [GPU Fan] [Mid Fan]                         |
+---------------------------------------------------------------+
```

On wide screens, CPU and GPU gauges must be side by side.

On narrow screens, they may stack vertically.

---

## Required UI changes

### 1. Make CPU/GPU gauges bigger

Increase the visual size of the CPU and GPU gauge widgets.

Target sizing:

- preferred width: around `260–320 px`
- preferred height: around `220–280 px`
- larger central temperature number
- larger label
- extra line for MHz/speed
- thicker gauge arc
- improved tick spacing

Do not make them so large that they push fan cards below the fold on normal 1080p/1200p laptop screens.

Use GTK layout constraints rather than hardcoded absolute positioning.

Suggested implementation:

- If using `DrawingArea`, call `set_content_width()` and `set_content_height()` with larger preferred sizes.
- Use `hexpand = true` where appropriate.
- Put both gauges in a horizontal `gtk::Box` or responsive grid.
- Wrap with `adw::Clamp` or similar if the project already uses it.

### 2. Put CPU and GPU gauges side by side

Create a dedicated gauge row.

Suggested layout:

```rust
let gauge_row = gtk::Box::new(gtk::Orientation::Horizontal, 16);
gauge_row.add_css_class("fans-gauge-row");
gauge_row.set_homogeneous(true);
gauge_row.append(&gpu_gauge_card);
gauge_row.append(&cpu_gauge_card);
```

For responsive/narrow layouts:

- If the project already has width breakpoint handling, use it.
- Otherwise, use a `gtk::FlowBox` or `gtk::Grid` with wrapping behaviour.
- `gtk::FlowBox` is acceptable for gauge cards because it can naturally wrap.
- Avoid horizontal clipping.

Recommended:

```text
Wide:
[ GPU Gauge ][ CPU Gauge ]

Narrow:
[ GPU Gauge ]
[ CPU Gauge ]
```

### 3. Add operating speed / MHz display

Each gauge card must show temperature and operating speed.

#### CPU speed

Use existing CPU telemetry if available.

Look for existing CPU telemetry fields such as:

- `avg_freq_mhz`
- `current_freq_mhz`
- CPU average clock
- per-core frequency summary

The DBus/API/docs may already expose CPU telemetry keys including `avg_freq_mhz`. Use existing daemon/UI state if it is already parsed.

Display examples:

```text
Avg Clock: 4300 MHz
Avg Clock: -- MHz
```

For CPU, prefer average clock in MHz:

```text
CPU Avg Clock: 4300 MHz
```

or inside the gauge card:

```text
Avg Clock
4300 MHz
```

#### GPU speed

Check if GPU clock telemetry already exists. If not, add it properly through the provider/daemon path.

Do not query `nvidia-smi` directly from the UI.

If GPU clock is not currently available, implement this if it fits the project safely:

- Extend the NVIDIA provider to read GPU graphics/core clock and memory clock using `nvidia-smi`.
- Use a timeout and graceful fallback.
- Do not fail the app if `nvidia-smi` is missing.
- Expose values through daemon telemetry and UI state.

Suggested `nvidia-smi` query:

```bash
nvidia-smi --query-gpu=clocks.gr,clocks.mem --format=csv,noheader,nounits
```

Suggested telemetry fields:

```text
gpu_core_clock_mhz
gpu_memory_clock_mhz
```

Display examples:

```text
Core Clock: 2100 MHz
Memory Clock: 7000 MHz
Core Clock: -- MHz
```

If only one value can be shown in the gauge card, show GPU core/graphics clock first:

```text
Core Clock: 2100 MHz
```

Optionally show memory clock as a smaller secondary line:

```text
Memory: 7000 MHz
```

If the laptop has an AMD or Intel GPU and no clock provider exists, show:

```text
Core Clock: -- MHz
```

Do not invent GPU MHz values.

### 4. Add richer gauge card content

Each CPU/GPU gauge card should include:

- gauge visual
- device label: `GPU` / `CPU`
- temperature in °C
- speed/frequency line
- optional small status line

Suggested card layout:

```text
+--------------------------------+
| GPU                            |
|       circular gauge           |
|          48°C                  |
| Temperature                    |
| Core Clock: 2100 MHz           |
| Memory Clock: 7000 MHz         |
+--------------------------------+
```

```text
+--------------------------------+
| CPU                            |
|       circular gauge           |
|          64°C                  |
| Temperature                    |
| Avg Clock: 4300 MHz            |
| Package Power: 45 W optional   |
+--------------------------------+
```

Package power is optional if already available and easy to show.

### 5. Improve gauge drawing

Update the circular gauge drawing helper if needed.

Requirements:

- Larger arc radius.
- Thicker arc.
- Clear ticks.
- Smooth arc progress.
- Central value must not overlap labels.
- MHz text must not overlap temperature text.
- Temperature colour/state should still work:
  - normal below 70°C
  - warm 70–85°C
  - hot 85°C+
- GPU and CPU should have distinct accent styles:
  - GPU: blue/cyan accent
  - CPU: magenta/pink accent
- But do not hardcode unreadable colours in light mode.
- Ensure labels remain readable in both dark and light themes.

If the existing drawing widget does not support MHz text, extend it with a subtitle/metric field.

Suggested data structure:

```rust
struct GaugeMetric {
    label: String,
    temp_c: Option<f64>,
    primary_speed_label: String,
    primary_speed_mhz: Option<u64>,
    secondary_speed_label: Option<String>,
    secondary_speed_mhz: Option<u64>,
    accent: GaugeAccent,
}
```

or adapt to the existing code style.

### 6. Reposition the Cooling Mode card

The current Cooling Mode card appears to the right of small stacked gauges. After placing CPU and GPU gauges side by side, move Cooling Mode into a cleaner section.

Preferred:

```text
[ GPU Gauge ][ CPU Gauge ]

[ Cooling Mode / Controls Summary full width ]

[ Fan cards grid ]
```

Alternative for very wide screens:

```text
[ GPU Gauge ][ CPU Gauge ][ Cooling Mode ]
```

But the first option is safer and cleaner.

The Cooling Mode card should remain:

- clear
- readable
- not oversized
- not visually competing with the CPU/GPU gauges

### 7. Clean up fan rotor cards if needed

In the screenshot, the fan rotor cards show RPM text both inside/near the rotor and below the rotor. Avoid visual duplication or overlap.

Improve fan card display:

- animated fan rotor visual
- large RPM below rotor
- label below RPM
- status badge below label
- no overlapping text inside the rotor unless it is intentionally designed and readable

Recommended:

```text
[rotating fan visual]

2940 RPM
Fan 1

[Mapping uncertain]
```

Do not display a second RPM number inside the fan rotor if it makes the UI crowded.

### 8. Preserve read-only state handling

The current backend is read-only:

```text
backend: read-only
manual_percent: false
manual_rpm_target: false
curves: false
boost: false
```

The redesigned UI must still show:

- RPM telemetry is available
- controls are read-only
- no manual slider enabled
- no boost enabled
- no curves enabled
- mapping warning where needed

The UI must not show fake control availability just because the dashboard looks better.

### 9. Add or update CSS

Add/update CSS classes for the larger gauge layout.

Suggested classes:

```css
.fans-performance-dashboard
.fans-gauge-row
.fans-gauge-card
.fans-gauge-card-large
.fans-gauge-title
.fans-gauge-temp
.fans-gauge-speed
.fans-gauge-speed-muted
.fans-cooling-mode-card
.fans-rotor-grid
.fan-rotor-card
```

Style goals:

- larger, cleaner cards
- good spacing between gauges
- less cramped hero section
- subtle glow only around important dashboard areas
- clear text hierarchy
- responsive behaviour

Do not overdo neon effects. Keep the app professional.

### 10. State parsing and telemetry extension

Inspect existing UI state parsing.

If CPU frequency is already in shared state:

- use it directly.

If CPU frequency exists in daemon state but is not parsed in UI:

- parse it and expose it to the Fans page.

If GPU clock is not in daemon state:

- extend provider/daemon/core model carefully.

Suggested files to inspect:

```text
crates/rog-core/src/model.rs
crates/rog-providers/src/nvidia_smi.rs
crates/rog-daemon/src/main.rs
crates/rog-ui/src/main.rs
crates/rog-ui/src/fans_page.rs
crates/rog-ui/src/fan_widgets.rs
docs/DBUS_API.md
docs/FEATURE_MATRIX.md
```

Do not add direct hardware/command calls in the UI.

---

## Documentation updates

Update docs if telemetry fields or UI behaviour changed.

Required if GPU/CPU speed telemetry fields are added:

### `docs/DBUS_API.md`

Document new telemetry keys:

```text
gpu_core_clock_mhz
gpu_memory_clock_mhz
```

If CPU avg frequency is already documented, no duplicate needed. If not, document current CPU/fan-page usage.

### `docs/FEATURE_MATRIX.md`

Update GPU telemetry row if GPU clocks are added.

### `docs/GUI_SPEC.md` and `docs/UI_PAGES.md`

Update Fans page description:

- large CPU/GPU gauges
- side-by-side layout
- operating speed/MHz display
- animated fan rotors
- capability-aware controls

### `docs/TROUBLESHOOTING.md`

Add a note:

- GPU speed may show unavailable if `nvidia-smi` is missing, unsupported, timed out, or the GPU is powered down.

---

## Testing requirements

Run:

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Manual run:

```bash
cargo run -p rog-daemon
cargo run -p rog-ui
cargo run -p rog-cli -- fans
cargo run -p rog-cli -- sensors
cargo run -p rog-cli -- caps
```

If GPU clock telemetry is added:

```bash
nvidia-smi --query-gpu=clocks.gr,clocks.mem --format=csv,noheader,nounits
```

Manual UI test checklist:

- CPU and GPU gauges are larger than before.
- CPU and GPU gauges appear side by side on wide windows.
- Gauges stack cleanly on narrow windows.
- CPU gauge shows temperature and average MHz when available.
- GPU gauge shows temperature and core/memory MHz when available.
- Missing MHz values show `-- MHz`, not fake data.
- Cooling Mode card is cleanly placed and not cramped.
- Fan cards still show 4 fans and live RPM.
- Fan rotor animation still works.
- Read-only controls remain disabled.
- Mapping uncertain warning remains visible.
- No UI overlap in 100%, 125%, or 150% display scaling.
- Dark theme is readable.
- Light theme remains acceptable.
- No clippy warnings.

---

## Acceptance criteria

This change is successful when:

1. The Fans page visually prioritises CPU/GPU thermal status with larger gauges.
2. CPU and GPU gauges are side by side on normal laptop screen widths.
3. The page shows operating MHz/speed for CPU and GPU where available.
4. The UI never invents speed data.
5. Fan RPM cards remain visible and clean.
6. The Cooling Mode card has a better position.
7. Read-only fan backend behaviour remains correct.
8. The design remains native GTK/libadwaita, not web-based.
9. Existing pages are not broken.
10. Full validation passes.

---

## Do not do these things

Do not:

- call `nvidia-smi` from the UI
- fake GPU MHz if unavailable
- fake fan percentage
- enable manual controls on read-only hardware
- copy the reference image exactly
- add ASUS copyrighted assets
- introduce Electron/WebKit/Tauri
- create layout that only works when maximised
- break existing daemon polling
- break existing fan diagnostics
- ignore light mode
- leave raw diagnostics dominating the dashboard

---

## Final response expected from Codex

After implementation, provide:

1. Summary of layout changes.
2. Files changed.
3. Whether CPU MHz used existing telemetry or required parsing changes.
4. Whether GPU MHz telemetry was added or left unavailable.
5. Explanation of responsive gauge layout.
6. Testing commands run and results.
7. Known limitations.

Use this feature summary:

```text
The Fans page hero now uses larger side-by-side CPU/GPU gauges with live temperature and operating MHz display, while keeping fan RPM cards, animated rotors, read-only safety handling, and diagnostics intact.
```
