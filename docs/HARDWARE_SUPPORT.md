# Hardware Support

This document tracks real hardware validation evidence for the repository.

Important rule:

- do not treat a feature, model, distro, or desktop environment as validated unless there is an actual record in the repository

The current code is capability-aware and degrades cleanly when services, permissions, or sysfs endpoints are missing, but that is not the same thing as broad hardware validation.

## Current Evidence Status

Recorded machine validation entries:

- none committed yet

Implication:

- the repository currently has implementation coverage, not hardware-certification coverage
- unless a machine record is added, treat that machine or scenario as untested

### Structured lighting implementation record (not physical validation)

| Field | G615JMR target status |
| --- | --- |
| Model / DMI | ASUS ROG Strix G16; observed board `G615JMR_G615JMR`, matched by the narrow `G615JM` prefix |
| Keyboard brightness | Detected through `/sys/class/leds/asus::kbd_backlight`; read-only target evidence recorded, write not physically retested |
| Keyboard RGB | Implemented in code through `native-aura-hid`; **not physically validated** |
| Keyboard zones | None exposed by the verified upstream capability record |
| Keyboard per-key RGB | Unsupported |
| ARGB / multi-zone | Unsupported |
| Lightbar / logo / rear lighting | Not claimed by this target mapping |
| Native identity | USB `0b05:19b6`, interface `00`, driver `asus` |
| Descriptor / report | SHA-256 `bdcf63294f0793588d96a966c08b1e28062b36b5fdf5d54e714b0102bf1e1094`; report ID `0x5d`; 63-byte payload / 64-byte write |
| Protocol | `asus-g615jm-laptop-aura-64` |
| Modes encoded | Static, Breathe, Rainbow Cycle, Rainbow Wave, Pulse |
| Backend priority | verified asusd -> verified native HID -> sysfs brightness -> unavailable |
| Readback | No reliable hardware effect-state readback; accepted writes are not physical confirmation |
| Physical Apply observed | **No** |

This record describes the implementation allow-list and existing read-only discovery evidence. It
does not satisfy the validation requirements below and must not be advertised as working hardware.

## Untested / Not Yet Validated

The scenarios below are the minimum release hardware checks that still need real evidence unless a linked record is added.

| Scenario | Current status | Evidence location |
| --- | --- | --- |
| `asusd` missing flow | Untested | none |
| `supergfxd` missing flow | Untested | none |
| CPU readable but not writable | Untested | none |
| CPU writable | Untested | none |
| keyboard backlight readable but not writable | Untested | none |
| G615JMR native Aura RGB physical Apply | Untested | no write/readback evidence committed |
| native Aura PolicyKit and asusd-conflict flows | Untested | automated tests only |
| fan count `0` | Untested | none |
| fan count `1` | Untested | none |
| fan count `2` | Untested | none |
| fan count `3+` | Untested | none |
| fan manual percent works | Untested | none |
| fan RPM target works | Untested | none |
| fan curve works | Untested | none |
| fan sync mode works | Untested | none |
| fan boost mode works | Untested | none |
| fan restore Auto works | Untested | none |
| hybrid CPU with physical cores != logical threads | Untested | none |
| tray visibility across desktop environments | Untested | none |

Update this table only when:

- a real test was run, and
- the supporting record was added to the repository

If a scenario is not represented in the available test fleet, mark it as:

- `Not represented in current test fleet`

and explain why in the notes column or linked record.

## What Counts As A Validation Record

A machine should only be treated as validated when all of the following are true:

- `rog-helperd` starts and exports the session DBus API
- the GTK UI starts and the main pages render without obvious layout breakage
- unsupported or blocked features explain why they are unavailable instead of acting fake-broken
- Diagnostics matches the actual machine state
- a completed validation record has been added to the repository

Use:

- [docs/HARDWARE_VALIDATION_TEMPLATE.md](HARDWARE_VALIDATION_TEMPLATE.md)

to create a real machine record after testing.

## Minimum Evidence To Capture Per Machine

Every real machine record should include:

- model
- distro
- kernel
- desktop environment
- CPU
- GPU
- `asusd` version or `missing`
- `supergfxd` version or `missing`
- tested date
- tester

Capture these commands:

```bash
cargo run -p rog-cli -- hardware-report > hardware-report.md
cargo run -p rog-cli -- services
cargo run -p rog-cli -- dbus --filter "asus|rog|aura|kbd|keyboard|led|rgb|supergfx|power|upower"
cargo run -p rog-cli -- sensors
cargo run -p rog-cli -- fans
cargo run -p rog-cli -- fan-caps
cargo run -p rog-cli -- caps
cargo run -p rog-cli -- lighting-diagnostics
busctl --user introspect io.github.roghelper.Daemon /io/github/roghelper/Daemon
```

`hardware-report` is the preferred starting point. It uses read-only probes and produces a
Markdown record containing model, distro, kernel, desktop/session, CPU/GPU, installed service
versions and states, provider capabilities, fan/sensor endpoints, mapping confidence, and
permission/dependency results. Review the output, fill in the manual runtime fields, attach the
remaining command/screenshot evidence, and do not treat generated capability output as proof that
a hardware write succeeded.

Capture these UI views when relevant:

- Dashboard warning area and quick controls
- CPU page
- GPU page
- Diagnostics page
- Lighting page when keyboard backlight or Aura/RGB support is exposed

When CPU or keyboard writes are blocked, also capture:

```bash
ls -l /sys/devices/system/cpu/cpufreq/policy*/scaling_governor
ls -l /sys/devices/system/cpu/cpufreq/policy*/energy_performance_preference
ls -l /sys/devices/system/cpu/cpufreq/policy*/scaling_{min,max}_freq
ls -l /sys/devices/system/cpu/intel_pstate/no_turbo
ls -l /sys/devices/system/cpu/cpu*/online
ls -l /sys/class/leds/*/brightness
find /sys/class/hwmon -maxdepth 3 -type f \( -name "fan*_input" -o -name "fan*_label" -o -name "pwm*" -o -name "fan*_target" \) -print
```

## Release Scenario Matrix

The rows below describe what the first release should eventually have evidence for. A single machine does not need to satisfy every row.

| Scenario | How to identify it | Expected behavior | Evidence to capture |
| --- | --- | --- | --- |
| `asusd` missing | stop or remove `asusd`, or use a machine without it | profile and charge-limit controls stay visible but clearly explain that `asusd` is required | `services`, `dbus`, `caps`, Dashboard screenshot, Diagnostics screenshot |
| `supergfxd` missing | stop or remove `supergfxd`, or use a machine without it | GPU mode controls stay visible but clearly explain that `supergfxd` is required | `services`, `dbus`, `caps`, GPU screenshot, Diagnostics screenshot |
| CPU readable but not writable | CPU telemetry works, but CPU sysfs files are not writable by the current user | CPU telemetry remains visible, affected controls are read-only, and Diagnostics lists the blocked paths | `caps`, CPU Diagnostics copy, CPU screenshot, CPU sysfs `ls -l` output |
| CPU writable | CPU control sysfs paths are writable for the daemon user | at least one quick control and one policy control apply successfully | `caps`, CPU screenshot before/after, Diagnostics copy |
| keyboard backlight readable but not writable | keyboard brightness is readable but the LED `brightness` file is not writable | current brightness is still visible and the control is clearly read-only | Dashboard or Lighting screenshot, `caps`, LED `ls -l` output |
| Aura/RGB exposed by asusd | `caps` reports `has_aura: true` and the lighting backend is `asusd-aura` | RGB picker and backend-reported lighting modes are enabled when writable; brightness still works through Aura or sysfs fallback | `caps`, `lighting-diagnostics`, `dbus --filter "asus\|rog\|aura\|kbd\|keyboard\|led\|rgb"`, Lighting screenshot, Diagnostics copy |
| G615JMR native Aura | lighting backend is `native-aura-hid` and diagnostics match the complete allow-list above | only single-target RGB and the five supported modes are shown; Apply requests PolicyKit; result says accepted without readback; ARGB/zones/per-key remain unavailable | before/after physical observation, `caps`, `lighting-diagnostics`, Lighting screenshot, PolicyKit success/denial, asusd conflict test |
| asusd present without Aura/RGB | asusd service exists, but `caps` reports `has_aura: false` | RGB picker stays disabled with a clear “not exposed by asusd” or brightness-only fallback message | `caps`, `lighting-diagnostics`, DBus introspection output, Lighting screenshot |
| fan count `0` | no usable `fan_rows` are detected | UI does not invent fan rows; Diagnostics explains that fan telemetry is unavailable | `sensors`, Dashboard screenshot, Diagnostics screenshot |
| fan count `1` | exactly one `fan_rows` entry is detected | UI shows one row with a friendly label or `Fan 1` fallback | `sensors`, Dashboard screenshot, Diagnostics screenshot |
| fan count `2` | exactly two `fan_rows` entries are detected | UI shows exactly two rows in deterministic order | `sensors`, Dashboard screenshot, Diagnostics screenshot |
| fan count `3+` | three or more `fan_rows` entries are detected | UI expands to all detected rows without assuming a fixed layout | `sensors`, Dashboard screenshot, Diagnostics screenshot |
| fan manual percent writable | `fan-caps` reports `has_fan_manual_percent: true` | Fans page enables percentage controls after acknowledgement; daemon applies through hwmon and Auto remains available | `fans`, `fan-caps`, Fans screenshot, before/after RPM |
| fan RPM target writable | `fan-caps` reports `has_fan_manual_rpm_target: true` | RPM target is available only for fans exposing writable `fanN_target` | `fans`, `fan-caps`, endpoint `ls -l` |
| fan curve backend available | `fan-caps` reports `has_fan_curves: true` | curve apply validates safe high-temperature points and rejects dangerous curves | `fan-caps`, DBus/API result, Fans screenshot |
| fan sync mode | more than one controllable fan is detected | sync applies only to controllable fans and keeps read-only fan telemetry visible | `fans`, Fans screenshot before/after |
| fan boost mode | `has_fan_boost: true` | boost runs at 100% for a selected duration, then restores Auto/BIOS mode | `fans`, Fans screenshot, after-timeout confirmation |
| fan restore Auto | any controllable fan backend | Return to Auto returns control to firmware/backend automatic mode | `fans`, Fans screenshot before/after |
| hybrid CPU topology | physical cores != logical threads | CPU page shows correct physical-core count, logical-thread count, and all logical CPU rows | `caps`, CPU screenshot, CPU Diagnostics copy |

## Fan-Control Validation Fields

Include these fields in each machine record when fan testing is relevant:

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

## Lighting Validation Fields

Include these fields whenever lighting is tested:

- brightness working and backend used
- RGB physically changes: yes/no/not tested
- supported modes physically observed
- ARGB/multi-zone and per-key claims (normally `unsupported` unless independently verified)
- VID, PID, interface, driver, descriptor hash, report ID/size, and protocol family
- selected backend and alternatives suppressed
- PolicyKit success, denial/cancellation, helper unavailable, and active-asusd conflict results
- returned apply outcome, explicitly noting whether hardware readback exists

## How To Add Real Evidence

After testing on a real machine:

1. Copy [docs/HARDWARE_VALIDATION_TEMPLATE.md](HARDWARE_VALIDATION_TEMPLATE.md) into your notes or into a new committed record.
2. Fill in the template using only observed results.
3. Add links or pasted summaries for command outputs and screenshots.
4. Update the `Recorded machine validation entries` section in this file.
5. Update the `Untested / Not yet validated` table for any scenario the new record actually covers.

## Suggested Record Naming

If you want to add separate machine records, use a predictable name such as:

- `docs/hardware-validation/YYYY-MM-DD-model-name.md`

If you prefer to keep a smaller doc set, you can also paste the filled template directly into this file under a new `## Recorded Machines` section. Either approach is acceptable as long as the evidence is committed and reviewable.

## Current Unknowns

These areas still need more real machine coverage:

- exact ASUS model compatibility for `asusd` profile and charge-limit flows
- exact ASUS model compatibility for `asusd` Aura/RGB lighting flows
- exact ASUS model compatibility for `supergfxd` mode switching flows
- distro and desktop-environment differences for tray visibility
- how often keyboard backlight sysfs is readable but not writable across distros
- how often CPU write access can be granted safely without custom local policy work

Until real records exist here, treat support claims as “implemented in code” rather than “verified across hardware.”
