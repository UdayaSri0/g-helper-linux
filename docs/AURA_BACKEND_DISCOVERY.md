# ASUS Aura / RGB Backend Discovery

Discovery date: 2026-08-15
Target: ASUS ROG Strix G16 `G615JMR_G615JMR`

## Result

ROG Helper now contains two narrow, verified Aura adapters:

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
