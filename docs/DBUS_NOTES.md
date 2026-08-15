# DBus Notes

## Rule (important)

Do not hardcode ASUS `asusd` / `supergfxd` interface names or object paths until verified from
introspection output on the target machine.

## Milestone 1

- Uses standard UPower system bus APIs (`org.freedesktop.UPower`) for power source + battery %.
- Exposes a small session bus API for the UI:
  - name: `io.github.roghelper.Daemon`
  - path: `/io/github/roghelper/Daemon`
  - iface: `io.github.roghelper.Daemon1`
  - methods return `a{sv}` dictionaries for forward/backward compatibility (no hardcoded enum DBus types yet)
  - includes `SetLighting` (Milestone 1: keyboard backlight brightness when available via sysfs)

## Phase 0 Discovery Commands

Run these on the target laptop and paste the output before implementing `asusd`/`supergfxd` calls:

If `rg` is not installed, either install `ripgrep` or replace `rg` with `grep -E`.

```bash
busctl --system list | rg -i "asus|rog|supergfx|power|upower"
systemctl status asusd
systemctl status supergfxd
ls /sys/class/hwmon
grep -R . /sys/class/hwmon/hwmon*/name | head -n 50
ls /sys/class/leds | rg -i "kbd|asus"
```

## Fan-control discovery rule

Fan RPM telemetry, a candidate PWM file, writable Unix mode bits, and a readable curve are four
different capabilities. None of the first three alone authorizes control. Fan writes remain disabled
until an installed service or a backend-specific kernel ABI has been verified on the target, including
channel mapping, supported ranges, failure behavior, and a tested firmware/BIOS Auto restore path.

The August 2026 target-machine findings and implementation gate are recorded in
[`FAN_CONTROL_DISCOVERY.md`](FAN_CONTROL_DISCOVERY.md).

## Aura/RGB discovery rule

Aura-looking service, interface, method, or property names are not a control contract. Walk from the
standard DBus root, record exact signatures/access, and enable a provider only for a reviewed exact
match backed by real-device fixtures and readback. Never infer RGB from a model name, an ASUS HID
identifier, or the presence of keyboard brightness.

The August 2026 target has only a read-only sysfs brightness endpoint and no installed Aura service.
See [`AURA_BACKEND_DISCOVERY.md`](AURA_BACKEND_DISCOVERY.md).
