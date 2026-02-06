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
