# rog-helper (working name)

Linux-native control app (tray + window) for ASUS ROG laptops using the existing ASUS Linux ecosystem.

This repo is a Rust workspace:

- `crates/rog-core`: domain types + validation + policy engine (no I/O)
- `crates/rog-providers`: DBus clients + sysfs readers (UPower + hwmon implemented for Milestone 1)
- `crates/rog-daemon`: `rog-helperd` user daemon (session DBus API)
- `crates/rog-ui`: `rog-helper-ui` GTK4/libadwaita UI + tray (SNI via `ksni`)
- `crates/rog-cli`: `rog-helper` diagnostics CLI

See:

- `docs/BUILD.md`
- `docs/ARCHITECTURE.md`
- `docs/TROUBLESHOOTING.md`

