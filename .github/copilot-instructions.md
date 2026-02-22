# Copilot Instructions for rog-helper

## Project Overview
`rog-helper` is a Rust workspace that provides a Linux control application for ASUS ROG laptops. It's split into **four distinct layers** with unidirectional dependencies: **rog-core → rog-providers → rog-daemon → rog-ui/rog-cli**.

## Architecture & Layer Boundaries
- **rog-core** (`crates/rog-core`): Pure domain types, validation, and policy logic. **No hardware I/O—never add I/O here.** Home to `RogError`, `AppState`, `TelemetrySnapshot`, `PolicyState`.
- **rog-providers** (`crates/rog-providers`): Hardware access—DBus clients (`asusd`, `supergfxd`, `UPower`) + sysfs/hwmon readers. Implements provider traits that rog-core defines.
- **rog-daemon** (`crates/rog-daemon`): User session service (systemd --user). Aggregates providers, manages state, exports session DBus API (`io.github.roghelper.Daemon1`).
- **rog-ui** (`crates/rog-ui`): GTK4/libadwaita desktop app + system tray (`ksni`). Calls daemon via session DBus.
- **rog-cli** (`crates/rog-cli`): Diagnostics CLI; probes system DBus, hwmon, and systemd services.

## Error Handling
All crates use `rog_core::RogError` enum with variants:
- `DependencyMissing` (system DBus services unavailable)
- `NotSupported` (hardware feature not present)
- `PermissionDenied` (needs elevation)
- `InvalidInput` (validation failed)
- `TransientFailure` (temporary network/I/O error)
- `Unexpected` (catch-all)

Use the appropriate variant; avoid generic `Err()`. See `crates/rog-core/src/error.rs`.

## Async & Concurrency
- All daemon/provider code is async (tokio). Entry points: `#[tokio::main]`.
- Shared mutable state uses `Arc<RwLock<T>>` (e.g., daemon's `SharedState`). **Always use `.read()` or `.write()`; never block.**
- DBus calls may timeout; wrap with `tokio::time::timeout`.

## DBus & IPC
- **Daemon exports**: `io.github.roghelper.Daemon1` on session bus with methods like `GetTelemetry()`, `SetLighting()`.
- **UI subscribes** via `#[zbus::proxy]` trait; see `rog-ui/src/main.rs` lines 16–27.
- State & telemetry cross DBus as `HashMap<String, OwnedValue>` (serialized from `serde` types).
- Version: zbus v5 (check `[workspace.dependencies]` in `Cargo.toml`).

## Policy Engine Pattern
`rog-core::PolicyState` handles automation debouncing:
1. Input: `PolicyEvent` (e.g., power source change)
2. Outputs: `Vec<PolicyAction>` to apply
3. Enforces debounce window (default 5s) and manual-override state
4. See `crates/rog-core/src/policy.rs` for tests showing debounce logic.

## Development Workflow

**Build:**
```bash
cargo build --workspace
```

**Run locally (dev):**
- Terminal 1: `cargo run -p rog-daemon`
- Terminal 2: `cargo run -p rog-ui`
- Terminal 3: `cargo run -p rog-cli -- services` (diagnostics)

**Verify daemon DBus:** (requires daemon running)
```bash
busctl --user introspect io.github.roghelper.Daemon /io/github/roghelper/Daemon
busctl --user call io.github.roghelper.Daemon /io/github/roghelper/Daemon io.github.roghelper.Daemon1 GetTelemetry
```

**Install & systemd:**
```bash
cargo install --path crates/rog-daemon --bin rog-helperd --locked
mkdir -p ~/.config/systemd/user && cp packaging/systemd-user/rog-helperd.service ~/.config/systemd/user/
systemctl --user enable --now rog-helperd
```

## Project Conventions
- **Logging**: Use `tracing::{info, warn, debug}` (initialized in `main.rs`), never `println!`.
- **Serialization**: `serde` for domain types; especially `HashMap<String, serde_json::Value>` or `OwnedValue` for DBus.
- **Configuration**: No config files yet; automation rules encoded in `PolicyState`.
- **Dependency versions**: Check `[workspace.dependencies]` in root `Cargo.toml` for canonical versions (e.g., tokio 1, zbus 5).

## Common Tasks

**Adding a provider (new hardware interface):**
1. Add module in `crates/rog-providers/src/` (e.g., `new_sensor.rs`)
2. Implement provider trait if applicable
3. Integrate into daemon's telemetry loop (see daemon's `main.rs` telemetry collection)

**Adding a daemon DBus method:**
1. Add method to daemon struct in `crates/rog-daemon/src/main.rs`
2. Annotate with `#[interface(name = "io.github.roghelper.Daemon1")]`
3. Update UI proxy (`rog-ui/src/main.rs`) to call it

**Adding a domain type or validation:**
1. Add to `crates/rog-core/src/model.rs` or `policy.rs`
2. Include `Serialize`/`Deserialize` derives
3. Add validation tests in the module

## Key Files to Know
- `docs/ARCHITECTURE.md` – high-level design
- `docs/BUILD.md` – build & run recipes
- `crates/rog-core/src/error.rs` – error types
- `crates/rog-core/src/policy.rs` – automation debounce logic & tests
- `crates/rog-daemon/src/main.rs` – daemon entry, DBus interface, state aggregation
- `crates/rog-ui/src/main.rs` – UI entry, panel/tray, daemon proxy setup
