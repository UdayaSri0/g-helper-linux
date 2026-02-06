# Troubleshooting

## Daemon not reachable in UI

The UI talks to `rog-helperd` over the *session* bus.

1. Start the daemon:

```bash
cargo run -p rog-daemon
```

2. Then start the UI:

```bash
cargo run -p rog-ui
```

You can also verify the daemon is on the user bus:

```bash
busctl --user list | rg -n "io\\.github\\.roghelper\\.Daemon"
busctl --user call io.github.roghelper.Daemon /io/github/roghelper/Daemon io.github.roghelper.Daemon1 GetTelemetry
```

## Tray icon not visible

The tray uses StatusNotifierItem (SNI). Some desktop environments (notably GNOME) hide tray icons
unless an AppIndicator/SNI extension is enabled.

## Service discovery

Use the CLI:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- dbus --filter "asus|rog|supergfx|power|upower"
```

## Sensors missing

Some platforms expose limited sensors via `hwmon`, especially for NVIDIA dGPU.
Milestone 1 is best-effort and read-only.

## Lighting is read-only / cannot change RGB

`rog-helper` needs a supported backend to control lighting without running as root.

- Keyboard backlight via sysfs (`/sys/class/leds/asus::kbd_backlight`) is commonly **root-owned**
  (you can read it, but you cannot write `brightness` as a normal user).
- Full RGB/Aura modes (colors/effects) generally require `asusd` (from the `asusctl` project) on the
  system bus.

Next steps (recommended order):

1. Install and start `asusd` (distro package or build from source), then re-run diagnostics:
   - `cargo run -p rog-cli -- services`
   - `cargo run -p rog-cli -- dbus --filter "asus|rog"`
2. If you want to control keyboard brightness via sysfs instead, add a udev rule to grant your user
   write access to the LED brightness attribute (one-time root setup).
