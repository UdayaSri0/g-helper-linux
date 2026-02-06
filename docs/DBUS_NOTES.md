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
