# Architecture

The project is split into three layers:

1. UI (`rog-helper-ui`): unprivileged GTK4/libadwaita app + tray.
2. Daemon (`rog-helperd`): unprivileged user service; automation + state; exports a session-DBus API for the UI.
3. Providers (`rog-providers`): system DBus clients (asusd/supergfxd/upower) + sysfs/hwmon readers.

Data flow:

UI  <-> session DBus <->  Daemon  <-> system DBus <->  asusd / supergfxd
                                  |
                                  +-> sysfs/hwmon + UPower (telemetry)

Hardware I/O must never be implemented in the UI.

