# ASUS Aura / RGB Backend Discovery

Discovery date: 2026-08-15
Target: ASUS ROG Strix G16 `G615JMR_G615JMR`

## Result

ROG Helper now contains two narrow, contract-verified and fixture-tested Aura adapters:

1. the current `xyz.ljones.Asusd` Aura DBus ABI used by asusd 6.3.8 through 6.4.0;
2. a native HID adapter for the exact G615JMR target identity recorded below. The matcher uses the
   observed board-name prefix `G615JM`; this is an identity gate, not a support claim for other models.

The target's native contract is ordinary single-target RGB, not ARGB. It supports no independently
addressable zones and no per-key RGB. The implementation has deterministic fixtures and simulated
sysfs/DBus tests, but **has not yet been physically applied and observed on the target laptop**.
Treat the feature as implemented in code, not hardware-validated.

The verified fallback is the kernel LED-class keyboard backlight:

- device: `/sys/class/leds/asus::kbd_backlight`
- current brightness: `0`
- maximum brightness: `3`
- current-user access: readable, not writable (`root:root`, mode `0644`)
- honest modes: `Off` (brightness zero) and `Static` (non-zero brightness)

The original target discovery was read-only. No physical RGB write has been performed as part of
the repository evidence.

An installed-target audit on 2026-08-21 found why the then-installed UI remained read-only despite
this discovery: the installed helper exposed privileged API v1 without `SetAuraEffect`, while the
current daemon requires API v2, and that package lacked both `60-rog-helper-aura.rules` and
`/dev/rog-helper-aura`. D-Bus activation and the lighting PolicyKit action were present. Current
source packaging includes and validates the API-v2 helper, narrow udev rule, D-Bus/systemd files,
PolicyKit policy, and sandbox device allow-list; reinstalling current source/package is required to
replace an older installed payload.

## Evidence

Read-only inspection found:

- no ASUS, Aura, or supergfx well-known name on the system or user bus;
- `asusd.service` and `supergfxd.service` are not installed and are inactive;
- no `asusd`, `asusctl`, or `supergfxctl` executable in `PATH`;
- no Aura, RGB, effect, speed, or zone attribute below the target's `asus-nb-wmi` sysfs device;
- an ASUS N-KEY HID device (`0B05:19B6`) bound to the kernel ASUS driver.

Subsequent upstream review established this exact native allow-list:

| Field | Verified value |
| --- | --- |
| DMI board match | prefix `G615JM` (observed target `G615JMR_G615JMR`) |
| USB VID:PID | `0b05:19b6` |
| USB interface | `00` |
| expected driver | `asus` |
| report descriptor SHA-256 | `bdcf63294f0793588d96a966c08b1e28062b36b5fdf5d54e714b0102bf1e1094` |
| output report | ID `0x5d`, 63-byte payload / 64-byte write |
| local protocol name | `asus-g615jm-laptop-aura-64` |
| modes | Static, Breathe, Rainbow Cycle, Rainbow Wave, Pulse |
| speed | Slow, Medium, Fast where the mode supports it |
| direction | Right, Left, Up, Down for Rainbow Wave |
| secondary colour | Breathe only |
| zones / ARGB / per-key | unsupported / false / false |

Every field must match. A similar ASUS HID device remains diagnostic-only.

## Upstream basis and licence decision

Protocol and capability behavior was checked against the current ASUS Linux/asusctl repository at
HEAD [`1c456fa3`](https://gitlab.com/asus-linux/asusctl/-/commit/1c456fa3)
(`rog-aura/src/builtin_modes.rs`, `rog-aura/data/aura_support.ron`, and
`asusd/src/aura_laptop/mod.rs`). Upstream is MPL-2.0. ROG Helper therefore uses a small,
independently implemented compatible encoder and DBus adapter; it does not copy upstream source or
add a heavy runtime dependency. The captured asusd 6.3.8 ABI fixture records only interface shape.

The Prompt 02 comparison was repeated against asusctl commit
[`28b456de`](https://github.com/OpenGamingCollective/asusctl/commit/28b456dec2969e293e0d8eae7a8face629ec4be2)
(`rog-aura/src/builtin_modes.rs`, `rog-aura/src/usb.rs`, `asusd/src/aura_laptop/mod.rs`, and
`rog-aura/data/aura_support.ron`). Windows G-Helper was consulted only as a GPL-3.0 behavioral
reference at commit
[`54c5bd00`](https://github.com/seerge/g-helper/commit/54c5bd00da82e20a0361228e5758f692b3b9560b)
(`app/USB/Aura.cs` and `app/USB/AsusHid.cs`); no code was copied or translated. Public G615JMR
identity and Static-sequence observations are recorded in
[G-Helper discussion #4513](https://github.com/seerge/g-helper/discussions/4513).

### Built-in effect packet comparison

The fixed effect report is 64 bytes. The meaningful prefix and zero padding are:

| Offset | Meaning | Local encoding | Independent evidence / decision |
| ---: | --- | --- | --- |
| 0 | report ID | `5d` | asusctl, G-Helper, and target logs agree |
| 1 | effect command | `b3` | all sources agree |
| 2 | zone | `00` | whole target only; no zones are exposed locally |
| 3 | mode | Static `00`, Breathe `01`, Cycle `02`, Wave `03`, Pulse `0a` | asusctl independently agrees |
| 4..6 | primary RGB | requested RGB or zero when the mode has no colour | independently agreed field layout |
| 7 | speed | Slow `e1`, Medium `eb`, Fast `f5` | asusctl independently agrees |
| 8 | direction | Right `00`, Left `01`, Up `02`, Down `03` | only exposed for Rainbow Wave; physical target behavior remains unvalidated |
| 9 | effect/random/control flag | `00` | asusctl fixes this byte at zero. G-Helper uses other values in some cases, but that GPL-only observation has no independent G615JMR evidence, so bytes were not changed |
| 10..12 | secondary RGB | requested only for Breathe; otherwise zero | independently agreed field layout |
| 13..63 | reserved/padding | zero | asusd pads to the descriptor-sized 64-byte report |

The following reports remain `5d b5` plus zeros (SET), then `5d b4` plus zeros (APPLY). All three
implementations use EFFECT -> SET -> APPLY. No packet byte changed during this review. In particular,
the byte-9 investigation is documented uncertainty rather than a reason to adopt GPL-derived behavior.

## Implemented safety boundary

The DBus provider uses `org.freedesktop.DBus.ObjectManager` at `/`, accepts only service
`xyz.ljones.Asusd`, paths below `/xyz/ljones/aura/`, and interface `xyz.ljones.Aura`. The verified
6.3.8-6.4.0 ABI requires these exact properties:

- `LedModeData (uu(yyy)(yyy)ss)`, read/write
- `LedMode u`, read/write
- `SupportedBasicModes au`, read
- `SupportedBasicZones au`, read
- `AllModeData() -> a{u(uu(yyy)(yyy)ss)}`
- `DirectAddressingRaw(aay) -> ()`
- optional `Brightness u`, read/write, with optional `SupportedBrightness au`, read

The two method signatures are required as ABI fingerprints, but ROG Helper never calls
`DirectAddressingRaw`; all writes use structured `LedModeData`. `DeviceType u` is present in the
captured upstream fixture but is not used by the local adapter. Wrong names, paths, access modes,
or signatures remain diagnostic-only.

The native path is similarly closed. The udev rule creates `/dev/rog-helper-aura` without changing
the root-owned hidraw permissions. Privileged API v2 exposes only
`SetAuraEffect(mode, primary, secondary, speed, direction)`. It accepts no device path, raw bytes,
report ID, command ID, or zone. Immediately before a write the helper rechecks DMI, VID/PID,
interface discovery, driver, device number, open-file identity, descriptor hash, report shape, and
that exactly one supported target exists. It sends one fixed three-report effect/set/apply sequence,
does not retry a failed report, limits distinct requests to one per 250 ms, suppresses duplicates,
and bounds the operation to one second.

Readiness is reported as separate facts: hardware support, selected backend, helper
installed/reachable/compatible, PolicyKit availability, lighting category availability, and final
write-path readiness. `not_checked` authorization is an editable state; it is not read-only and
does not authenticate until Apply.

`rog-helper lighting-diagnostics` performs these checks without a hardware write. Its copy-friendly
report includes DMI, redacted canonical USB/interface identity and hashed physical identity,
descriptor length/hash, report shape, every candidate rejection reason, asusd ownership and verified
ABI (and explicitly states when its version is not exposed), selected backend, helper API version and
compatibility, PolicyKit/lighting-category availability, alias identity, consolidated write readiness,
and the physical-validation-record flag.

## Provider hierarchy

Selection is:

1. verified asusd Aura;
2. verified native Aura HID;
3. sysfs LED brightness;
4. unavailable.

If either `xyz.ljones.Asusd` or the legacy ASUS daemon name owns the system-bus name, native HID is
suppressed to avoid competing writers. The helper checks ownership again before and during the
fixed report sequence.

Aura and sysfs may be combined only when Aura lacks brightness and the LED path is present. Sysfs
never becomes an RGB backend.

## Validation still required

Before claiming the target works physically, record a supervised Apply for every exposed mode,
the supported speed/direction combinations, secondary colour for Breathe, PolicyKit denial and
success, duplicate suppression, asusd conflict suppression, helper absence, and failure behavior.
Because the HID protocol has no reliable effect-state readback, observation of the physical LEDs is
required; `accepted_no_readback` is not proof of a lighting change.

The supervised developer flow is dry-run by default:

```bash
cargo run -p rog-cli -- lighting-test --safe-sequence
```

After reviewing the exact identity, ownership, helper, alias, and PolicyKit preflight, a human may
explicitly run the fixed sequence with:

```bash
cargo run -p rog-cli -- lighting-test --safe-sequence --confirm-g615jmr-physical-write
```

It refuses identity ambiguity/mismatch, active ASUS-daemon ownership, helper/API/category failure,
missing PolicyKit, or alias mismatch. It sends only high-level `SetLighting` requests through the
session daemon, waits between effects, labels every successful call `accepted_no_readback`, prints a
human observation checklist, and attempts a neutral Static-white restore even after an intermediate
failure. Native state has no readback, so restoring a prior unknown physical state cannot be promised.
The command never edits this support record. A committed validation record must include date, kernel,
BIOS, package and commit, identity hashes, each requested effect/direction, transport acceptance,
human observation, and final restore observation.

For a different ASUS device or asusd contract, first collect read-only evidence:

```bash
busctl --system --no-pager list
busctl --system tree SERVICE
busctl --system introspect SERVICE OBJECT_PATH
systemctl show asusd.service -p LoadState -p ActiveState -p SubState
cargo run -p rog-cli -- lighting-diagnostics
```

Choose `SERVICE` from the owned well-known names and `OBJECT_PATH` from `busctl tree`; do not copy a
guessed path. Also include the service/package version and a description of the physical keyboard
zones. Redact unrelated bus data if needed.

**AURA / RGB WRITES IMPLEMENTED IN CODE: YES, FOR THE EXACT CONTRACTS ABOVE.**

**PHYSICAL TARGET VALIDATION: NO.**

**ARGB / ZONES / PER-KEY SUPPORT ON G615JMR: NO.**
