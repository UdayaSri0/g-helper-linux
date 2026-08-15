# ASUS Aura / RGB Backend Discovery

Discovery date: 2026-08-13  
Target: ASUS ROG Strix G16 `G615JMR_G615JMR`

## Result

No verified Aura/RGB control backend is available on this target. RGB colour, effects, effect
speed, and zones therefore remain unavailable. The daemon must not send Aura DBus calls or raw
HID reports on this machine.

The verified fallback is the kernel LED-class keyboard backlight:

- device: `/sys/class/leds/asus::kbd_backlight`
- current brightness: `0`
- maximum brightness: `3`
- current-user access: readable, not writable (`root:root`, mode `0644`)
- honest modes: `Off` (brightness zero) and `Static` (non-zero brightness)

This discovery was read-only. No lighting value, kernel module, permission, or service state was
changed.

## Evidence

Read-only inspection found:

- no ASUS, Aura, or supergfx well-known name on the system or user bus;
- `asusd.service` and `supergfxd.service` are not installed and are inactive;
- no `asusd`, `asusctl`, or `supergfxctl` executable in `PATH`;
- no Aura, RGB, effect, speed, or zone attribute below the target's `asus-nb-wmi` sysfs device;
- an ASUS N-KEY HID device (`0B05:19B6`) bound to the kernel ASUS driver.

The HID identity is hardware evidence, not a control protocol. It does not authorize guessed
`hidraw` packets, model-name capability assumptions, or RGB controls.

## Implemented safety boundary

The DBus discovery provider walks downward from the standard `/` object path and records exact
method input signatures plus property signatures/access. Aura-looking names are diagnostic
candidates only. They cannot construct a writable provider.

A future backend may be enabled only by an exact, reviewed match of:

1. owned DBus service name and service version;
2. introspected object path and interface name;
3. getter/setter method and property signatures;
4. backend-reported modes, zones, speeds, and numeric ranges;
5. successful readback and error behavior on the real device;
6. an unprivileged permission model appropriate for the session daemon;
7. captured introspection fixtures and provider tests for that exact contract.

Generic numeric effect IDs are deliberately not mapped to ASUS effect names. The daemon also
rejects unverified `speed` and `zone` requests and does not silently accept unsupported mode
writes.

## Provider hierarchy

When a verified contract is added, selection remains:

1. verified Aura backend for only the capabilities it actually reports;
2. sysfs LED backend for keyboard brightness and the basic Off/Static interpretation;
3. unavailable, with diagnostic evidence and no hardware write attempt.

The two backends may be combined only when Aura lacks brightness and the sysfs brightness path is
present. RGB support must never be inferred from the laptop name or from brightness telemetry.

## Data needed next

On a machine with a working ASUS lighting service, collect the following without invoking setters:

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

**AURA / RGB WRITES IMPLEMENTED: NO.** No installed service exposed a verifiable control contract;
enabling writes would require guessing a DBus or HID protocol.
