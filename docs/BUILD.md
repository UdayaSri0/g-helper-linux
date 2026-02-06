# Build

## Prereqs

- Rust toolchain (stable)
- System deps (names vary by distro):
  - GTK4 development packages
  - libadwaita development packages
  - pkg-config

Ubuntu/Debian example:

```bash
sudo apt-get update
sudo apt-get install -y build-essential pkg-config libgtk-4-dev libadwaita-1-dev
```

## Build

```bash
cargo build --workspace
```

## Run (dev)

Terminal 1:

```bash
cargo run -p rog-daemon
```

Terminal 2:

```bash
cargo run -p rog-ui
```

## Verify Daemon DBus API

With `rog-helperd` running:

If `rg` is not installed, either install `ripgrep` or replace `rg` with `grep -E`.

```bash
busctl --user list | rg -n "io\\.github\\.roghelper\\.Daemon"
busctl --user introspect io.github.roghelper.Daemon /io/github/roghelper/Daemon
busctl --user call io.github.roghelper.Daemon /io/github/roghelper/Daemon io.github.roghelper.Daemon1 GetTelemetry
```

## Install (Local)

Install binaries into `~/.cargo/bin`:

```bash
cargo install --path crates/rog-daemon --bin rog-helperd --locked
cargo install --path crates/rog-ui --bin rog-helper-ui --locked
cargo install --path crates/rog-cli --bin rog-helper --locked
```

## systemd --user

Install the user service unit and start the daemon:

```bash
mkdir -p ~/.config/systemd/user
cp packaging/systemd-user/rog-helperd.service ~/.config/systemd/user/
systemctl --user daemon-reload
systemctl --user enable --now rog-helperd
systemctl --user status rog-helperd --no-pager
```

CLI diagnostics:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- sensors
```
