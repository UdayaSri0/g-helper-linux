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

CLI diagnostics:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- sensors
```

