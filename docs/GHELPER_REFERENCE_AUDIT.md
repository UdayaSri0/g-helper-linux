# G-Helper Reference and Linux Baseline Audit

Audit date: 2026-09-30  
Linux baseline: `UdayaSri0/g-helper-linux` `0145a261dcbfe57bea452c3256d3b8caef49ff0b`
(`main`, version 0.3.1). The local checkout and GitHub `main` resolved to the same commit.

This is a source, test, packaging, and upstream-behavior audit. It performed no fan, lighting, or
other physical hardware writes. “Software-validated” below means deterministic tests or fixture
coverage; it does not mean physically validated on a laptop.

## Licensing and independent-implementation boundary

ROG Helper Linux is `MIT OR Apache-2.0`. Windows G-Helper is GPL-3.0. G-Helper was used only to
study behavior, workflows, protocol observations, and user-facing concepts. Its source must not be
copied, translated, or lightly rewritten into this repository.

Protocol facts must be independently corroborated with Linux kernel interfaces, asusctl/asusd,
descriptors, fixtures, or supervised hardware observation. Even when a numeric value agrees across
projects, the Linux implementation must keep its own capability checks, validation, error handling,
and tests.

## Pinned upstream sources

| Source | Revision reviewed | Relevant paths | License / role |
| --- | --- | --- | --- |
| [Windows G-Helper](https://github.com/seerge/g-helper) | [`54c5bd00da82e20a0361228e5758f692b3b9560b`](https://github.com/seerge/g-helper/commit/54c5bd00da82e20a0361228e5758f692b3b9560b) | `app/USB/AsusHid.cs`, `app/USB/Aura.cs`, `app/Peripherals/Keyboard/AsusKeyboard.cs`, `app/AsusACPI.cs`, `app/Fans.cs`, `app/Mode/ModeControl.cs`, `app/Display/ScreenControl.cs`, `app/Input/InputDispatcher.cs`, `app/Program.cs`, `app/UpdatesController.cs` | GPL-3.0; behavioral research only |
| [Legacy asus-linux/asusctl](https://gitlab.com/asus-linux/asusctl) | [`321b1114a0ac3782d67aa39201682effb749f4d9`](https://gitlab.com/asus-linux/asusctl/-/commit/321b1114a0ac3782d67aa39201682effb749f4d9) | migration notice and repository history | MPL-2.0; the repository now points development to the successor |
| [Current OpenGamingCollective/asusctl](https://github.com/OpenGamingCollective/asusctl) | [`28b456dec2969e293e0d8eae7a8face629ec4be2`](https://github.com/OpenGamingCollective/asusctl/commit/28b456dec2969e293e0d8eae7a8face629ec4be2) | `rog-aura/src/builtin_modes.rs`, `rog-aura/src/usb.rs`, `rog-aura/src/aura_detection.rs`, `rog-aura/README.md`, `asusd/src/aura_laptop/mod.rs`, `asusd/src/ctrl_platform.rs`, `rog-profiles/src/fan_curve_set.rs`, `rog-dbus/src/zbus_fan_curves.rs` | MPL-2.0; current Linux ASUS reference |
| [Linux kernel](https://github.com/torvalds/linux) | [`551c722f40809618230001baccf219193e22fc5a`](https://github.com/torvalds/linux/commit/551c722f40809618230001baccf219193e22fc5a) | `drivers/platform/x86/asus-wmi.c`, `drivers/platform/x86/asus-nb-wmi.c` | GPL-2.0-only; authoritative kernel ABI/reference |
| [Linux hwmon ABI](https://docs.kernel.org/hwmon/sysfs-interface.html) | documentation served at audit time; kernel commit above pins the reviewed driver | standard `fan*`, `pwm*`, and automatic-point vocabulary | authoritative ABI documentation |

The older Aura discovery used asusctl commit `1c456fa3`. That provenance remains historically useful,
but this audit uses the current successor revision above for present conclusions.

## Current Linux implementation

The classification separates implementation, backend availability, and physical validation.

| Area | Classification | Source truth and limits |
| --- | --- | --- |
| Architecture / process boundaries | Implemented and software-validated | GTK/libadwaita UI uses the session D-Bus daemon. The daemon prefers asusd, supergfxd, UPower, and safe direct providers. Only permission-denied supported writes may fall back to the path-free system-D-Bus helper and category-specific PolicyKit. |
| CPU controls | Implemented but backend-limited | Telemetry plus turbo, power preset, governor, EPP, frequency bounds, and logical-CPU online state use validated Linux sysfs. Direct writes are preferred; the helper uses typed operations and readback. Real-machine writable/read-only scenarios are not committed. |
| GPU mode | Implemented but backend-limited | Read/write and supported-mode probing use supergfxd. Pending, logout, reboot, unsafe, missing-service, and external authorization states are explicit. No direct PCI/module/ACPI fallback exists. |
| ASUS platform profiles | Implemented but backend-limited | Read/write uses asusd. The remembered last manual profile is inert and is not a named multi-control profile. |
| Battery limit | Implemented but backend-limited | asusd is preferred. Fallback requires one unambiguous `type=Battery` device with the standard `charge_control_end_threshold`; direct writes precede the typed helper and return actual readback. |
| Keyboard brightness | Implemented but backend-limited | Verified asusd, direct LED sysfs, then a typed helper restricted to the canonical ASUS keyboard LED. Physical target write validation is absent. |
| Aura / RGB | Implemented but hardware-unvalidated | Exact asusd 6.3.8–6.4.0 contract or the allow-listed G615JM HID identity. Native support is single-target RGB with Static, Breathe, Rainbow Cycle, Rainbow Wave, and Pulse; no ARGB, zones, or per-key claim. Transport acceptance has no state readback. |
| Fan telemetry | Implemented but backend-limited | Dynamic `fanN_input` discovery preserves unknown mapping and read-only rows. The 0/1/2/3+ real-hardware matrix is not committed. |
| Fan curves / Auto | Implemented but hardware-unvalidated | Only the exact ASUS WMI CPU/GPU/Mid mapping and complete eight-point curve ABI. Writes are validated, staged, read back, enabled last, and rolled back to Auto on failure. The helper owns a recovery marker. Runtime curve reading/editing is not exposed beyond the fixed conservative UI preview. |
| Fan sync | Partially implemented | Model, D-Bus, UI, and persistence exist, but sync changes only manual-percent routing. It does not synchronize curve application, and no verified manual-percent backend exists. |
| Generic PWM/manual percent | Deliberately unsupported for safety | Candidate paths remain diagnostic-only. The setter always rejects them even if a file appears writable. |
| RPM target | Deliberately unsupported for safety | `fanN_target` candidates are diagnostic-only; no approved backend exists. |
| Fan boost | Deliberately unsupported for safety | Dormant daemon/UI shape depends on manual-percent support, which is not advertised or implemented by a verified backend. |
| Persistent configuration | Implemented and software-validated, limited scope | Versioned XDG TOML, legacy migration, field-level fallback, unknown-field tolerance, and atomic replacement exist. Only UI/dashboard preferences and inert charge/profile/sync suggestions are stored. |
| Settings | Implemented | Lifecycle, dashboard, inert control preferences, automation-unavailable explanation, and confirmed reset are present. There is no rules editor. |
| Automation | Partially implemented | `rog-core` models debounce and manual override, but the daemon does not run it; no persistent rules or automatic hardware application exist. |
| Diagnostics | Implemented and software-validated | CLI and UI cover dependencies, permissions, capabilities, fan mapping, CPU access, lighting identity/contract, privileged readiness, and hardware-report generation without remediation writes. |
| Privileged helper / PolicyKit | Implemented and software/security-validated | Four categories—CPU, fan, lighting, battery—use typed, semantic, path-free methods. There is no arbitrary path, process, command, raw HID, or generic sysfs method. Physical authorization flows remain unvalidated. |
| systemd / udev | Implemented; installed-target retest needed | The root helper is sandboxed. The Aura rule creates only a root-owned alias; it grants no `uaccess` or broad mode. The recorded installed target predated helper API v2/current Aura packaging. |
| Packaging | Implemented, validation incomplete | Debian, RPM, Arch, Flatpak, AppImage, tarball, CI, and tagged-release flows exist. Broad cross-distro and AppImage runtime coverage remains incomplete. |
| Update flow | Implemented but limited | Manual GitHub-release check; direct replacement only for a matching writable user-local binary, otherwise the release page opens. It is not a package-manager updater and downloaded binaries lack checksum/signature verification in the UI flow. |
| Tray | Implemented but desktop-limited | StatusNotifierItem/AppIndicator via `ksni`; cross-desktop visibility is unvalidated. |
| Hotkeys | Missing | No global-shortcut/input backend or settings exist. |
| Display refresh | Missing | No DRM, Wayland, compositor, or XRandR provider/API/UI exists. |

Primary evidence is in `crates/rog-core/src/{config,model,policy,privileged}.rs`,
`crates/rog-providers/src/{asusd,aura,aura_hid,cpu,hwmon,kbd_backlight,power_supply,setup,supergfx}.rs`,
`crates/rog-daemon/src/main.rs`, `crates/rog-privileged/src/main.rs`,
`crates/rog-ui/src/main.rs`, `crates/rog-cli/src/main.rs`, and `packaging/`.

## Documentation drift reconciled

The following statements were stale because they described an earlier implementation phase or
confused a compatibility-shaped API with an operational backend:

- `ROADMAP.md` listed generic writable manual percentage, RPM target, sync, and boost as implemented.
  Source forces generic manual/RPM capability false and rejects both setters; boost depends on the
  rejected manual path. The roadmap now describes only verified ASUS WMI curve/Auto writes.
- `README.md` claimed “curve sync.” Curve apply targets one fan and ignores sync. That claim was
  removed; sync is now described as partially modeled and non-operational for the current backend.
- `ROADMAP.md` and `ARCHITECTURE.md` described fan curves as merely modeled. The exact ASUS WMI
  transaction, privileged fallback, recovery marker, and fixed Cooling apply flow are implemented in
  code, but remain physically unvalidated.
- `FEATURE_MATRIX.md` called sync implemented and said Settings contained automation policy. It now
  identifies sync as partial and the Settings automation row as explanation/status only.
- `HARDWARE_SUPPORT.md` called unsupported generic fan operations merely “untested.” It now separates
  deliberately unsupported operations from real hardware-validation work.
- `PRIVILEGED_SECURITY_REVIEW.md` said every lighting write has readback. Sysfs lighting does; native
  Aura does not and explicitly returns accepted-without-readback.
- README battery/keyboard summaries omitted the standard battery fallback and typed keyboard helper;
  they now match provider priority.

`FAN_CONTROL_DISCOVERY.md` and `AURA_BACKEND_DISCOVERY.md` were otherwise the most accurate existing
documents and retain their conservative physical-validation conclusions.

## Windows behavior and Linux mapping

### Aura / RGB

G-Helper uses ASUS VID plus explicit product IDs, HID report availability, and device-specific paths;
it distinguishes laptop single-zone/multi-zone/per-key devices, TUF/ACPI devices, Ally hardware,
external keyboards, rear lighting, older Strix variants, and Windows Dynamic Lighting. This is a
useful capability-model concept, not permission to match all ASUS HID devices.

Both G-Helper and current asusctl corroborate the built-in mode vocabulary and the fixed
`0x5d/0xb3` effect plus SET `0xb5` and APPLY `0xb4` framing. Current asusctl also corroborates
64-byte descriptor-sized writes and low/medium/high bytes `0xe1/0xeb/0xf5`. Direction differs in
the reviewed behavior: G-Helper’s laptop built-in path uses zero while asusctl models right/left/up/
down. Direction therefore remains a target-validation question, not a cross-upstream certainty.

The cross-referenced firmware mode IDs are Static `0`, Breathe `1`, Color/Rainbow Cycle `2`,
Rainbow Wave `3`, Star `4`, Rain `5`, Highlight `6`, Laser `7`, Ripple `8`, Strobe/Pulse `10`,
Comet `11`, and Flash `12`. This vocabulary does not authorize every mode on the G615JMR; the local
capability allow-list remains smaller.

Primary/secondary color, speed, random behavior, direction, direct/software effects, zones, and
device layout are target- and mode-specific. Software effects such as ambient, audio, heatmap, or
per-key maps require layout maps, bounded streaming, cancellation, and ownership arbitration; they
must not be enabled from protocol resemblance alone. External keyboard probing is a separate device
family and is out of scope for the internal G615 target.

Diagnostics concepts worth adopting are redacted VID/PID/interface, descriptor/report sizes,
report-ID presence, capability flags, ownership, and accepted-versus-observed state. A raw report
writer is not acceptable.

### Fans

Linux `asus-wmi.c` is authoritative for the current path: eight CPU/GPU/Mid points, byte-valued
temperatures, hwmon PWM `0..255`, cached staging, and explicit enable/reset behavior. Profile writes
invalidate custom curves. Current asusd consequently reapplies a stored profile’s curve after the
platform-profile write. Any future combined profile operation must therefore apply profile first,
then its fan curves, with Auto rollback on failure.

G-Helper offers CPU/GPU/Mid charts, eight editable points, reset to BIOS defaults, named/custom
profiles, calibration, and hysteresis. The editor concepts are useful; calibration and hysteresis
are not. The reviewed mainline kernel exposes no matching hysteresis attribute, so implementing the
Windows WMI call would bypass the Linux ABI and cross the licensing/safety boundary.

ROG Helper Linux intentionally keeps stricter curve validation than current asusctl: strictly
increasing temperatures, non-decreasing duty, and conservative hot-end floors. Kernel/cache
readback is necessary but does not prove physical fan behavior.

### Other concepts

- AC/battery automation: adopt debounce, idempotence, ordered semantic actions, explicit manual
  override, and per-source rules using UPower/logind/platform-profile/asusd.
- Display refresh: research a session-scoped compositor/DRM backend; never port Windows display APIs
  or assume one internal panel.
- Keyboard timeout: use desktop idle/session APIs with lock and suspend handling, not global input
  snooping.
- Hotkeys: prefer standard kernel keycodes and desktop portal/session shortcuts; invoke only typed
  daemon operations.
- GPU/profile coordination: keep asusd/supergfxd authoritative and surface pending/reboot/logout
  state rather than creating direct ACPI/PCI operations.
- Updates: use distro package managers and `fwupd` concepts for system firmware; do not port Windows
  driver inventory/update APIs.

## Reuse decision matrix

| Feature / concept | Decision | Intended Linux-native backend or reason |
| --- | --- | --- |
| Capability-driven lighting UI | Adopt concept now | Existing daemon capability maps; keep deny-by-default behavior |
| Exact HID identity/descriptor diagnostics | Adopt concept now | Linux sysfs/hidraw metadata, read-only CLI diagnostics |
| G615JM fixed semantic Aura effects | Research first | Existing typed native HID helper; supervised physical observation required |
| Broader Aura product IDs | Research first | Per-device DMI/USB/interface/driver/descriptor evidence and fixtures |
| Aura primary/secondary color and speed | Research first | Existing semantic encoder, per-mode target observation |
| Aura direction | Research first | Upstreams do not establish identical behavior for the target |
| Random flag / black-as-random | Research first | Protocol quirk must not become generic UI behavior without observation |
| Direct/per-key/software effects | Out of scope | No verified target layout/streaming contract; high ownership and rate risk |
| External keyboards/peripherals | Research first | P3: separate identity, interfaces, acknowledgements, maps, and ownership |
| ASUS WMI eight-point curves and Auto | Linux equivalent exists | Existing exact Linux hwmon backend and typed helper; validate hardware |
| Profile-linked persistent curves | Research first | After P0: platform profile first, curve reapply second; needs transaction/rollback design |
| Generic PWM/manual percent | Unsafe / insufficiently verified | No approved Linux backend; writable appearance is insufficient |
| RPM target | Unsafe / insufficiently verified | No approved backend contract |
| Fan boost | Unsafe / insufficiently verified | Depends on unverified manual-duty semantics |
| Fan calibration/hysteresis | Unsafe / insufficiently verified | No verified Linux ABI; do not reproduce private Windows WMI calls |
| Named hardware profiles | Adopt concept now | After P0: versioned XDG config plus capability-backed semantic daemon operations |
| AC/battery automation | Adopt concept now | After profiles: UPower/logind plus daemon policy runtime; default off |
| Display refresh automation | Research first | Compositor/DRM/session backend, then policy engine |
| Keyboard timeout | Research first | Desktop idle/session API |
| Global hotkeys | Research first | Desktop portal/session shortcut and standard Linux input keycodes |
| Windows ACPI `DeviceIoControl` | Not applicable on Linux | Use kernel/asusd/supergfxd interfaces only |
| Windows service/elevation model | Linux equivalent exists | Preserve session D-Bus plus narrow system D-Bus/PolicyKit helper |
| Windows driver/BIOS updater | Not applicable on Linux | Distro package manager and `fwupd` |

## Unresolved questions

1. Does every advertised native Aura mode, speed, secondary-color combination, and wave direction
   visibly behave as encoded on the physical G615JMR?
2. Is the `G615JM` DMI prefix sufficiently narrow, or must the allow-list require an exact board
   string/suffix set after additional read-only evidence?
3. How does native Aura behave across suspend/resume, helper failure, and asusd ownership changes?
4. Do all three ASUS WMI fan channels apply, read back, restore Auto, survive helper restart recovery,
   and interact safely with suspend/resume and firmware profile changes?
5. Should fan-sync capability be disabled immediately for curve-only fans, or later gain an explicit
   multi-curve transaction with rollback semantics?
6. Which compositor/session API can provide portable read-only refresh discovery before any display
   write is considered?
7. Which desktop shortcut and idle APIs cover the supported environments without global input
   interception or root access?
8. Should the best-effort user-local updater verify published checksums or signatures before any
   future release-readiness claim?

The dependency-ordered answer to these questions is maintained in
[IMPLEMENTATION_PRIORITY.md](IMPLEMENTATION_PRIORITY.md).
