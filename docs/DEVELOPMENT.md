# Development

This document is for contributors working on the current repository state.

For a shorter entry point with run commands, source-file map, and grouped docs links, start with [QUICK_START.md](QUICK_START.md).

## Repo Layout

Top-level structure:

- `Cargo.toml`
  - workspace definition
- `crates/rog-core`
  - shared types, validation, errors, policy model
- `crates/rog-providers`
  - integration layer for DBus, sysfs, procfs, and command fallback
- `crates/rog-daemon`
  - session daemon and DBus service
- `crates/rog-privileged`
  - optional root system-DBus service and PolicyKit authorization boundary
- `crates/rog-ui`
  - GTK/libadwaita desktop UI and tray
- `crates/rog-cli`
  - diagnostics CLI
- `docs/`
  - project documentation
- `packaging/`
  - desktop entry, session/system DBus activation, systemd units, PolicyKit policy, AppStream metadata, and release scripts

## How the Current Crates Are Structured

### `rog-core`

Focus here when working on:

- shared domain model changes
- validation
- error types
- policy-model changes

Key files:

- `crates/rog-core/src/model.rs`
- `crates/rog-core/src/error.rs`
- `crates/rog-core/src/policy.rs`

### `rog-providers`

Focus here when working on:

- new backends
- DBus integration
- sysfs or procfs integration
- telemetry sources
- platform-specific capability probing

Key files:

- `crates/rog-providers/src/asusd.rs`
- `crates/rog-providers/src/supergfx.rs`
- `crates/rog-providers/src/cpu.rs`
- `crates/rog-providers/src/hwmon.rs`
- `crates/rog-providers/src/memory.rs`
- `crates/rog-providers/src/dbus.rs`

### `rog-daemon`

Focus here when working on:

- session DBus API changes
- state ownership
- telemetry loop
- action routing
- capability aggregation

Key file:

- `crates/rog-daemon/src/main.rs`

Important current note:

- most of the daemon is still in a single source file

### `rog-ui`

Focus here when working on:

- page layout
- tray behavior
- capability-driven control states
- daemon payload decoding
- action flows and UX

Key files:

- `crates/rog-ui/src/main.rs`
- `crates/rog-ui/src/shell.rs`
- `crates/rog-ui/src/theme.rs`
- `crates/rog-ui/src/widgets/mod.rs`
- `crates/rog-ui/src/fan_widgets.rs`

Important current note:

- state decoding, background actions, refresh wiring, and most page construction remain in `main.rs`; the desktop shell, design system, reusable cards/headers/graphs, and fan drawing are separated

### `rog-cli`

Focus here when working on:

- diagnostics workflows
- environment and DBus probing
- field support tooling

Key file:

- `crates/rog-cli/src/main.rs`

## Common Contributor Workflow

1. Read the relevant crate and module first.
2. Check whether the behavior is already documented in `docs/`.
3. Prefer the source when docs and code disagree.
4. Make changes in the smallest layer that can correctly own them:
   - domain change -> `rog-core`
   - backend change -> `rog-providers`
   - API/state change -> `rog-daemon`
   - root-only approved operation -> `rog-privileged`, with shared contract in `rog-core`
   - presentation / UX change -> `rog-ui`
   - troubleshooting / probing improvement -> `rog-cli`
5. Update docs in the same change when the behavior or surface area changes.

## Build and Validation Commands

Current useful commands:

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

The CI workflow currently runs the same validation set.

## Running the Project Locally

Typical local workflow:

```bash
cargo run -p rog-daemon
```

In another terminal:

```bash
cargo run -p rog-ui
```

Optional diagnostics:

```bash
cargo run -p rog-cli -- services
cargo run -p rog-cli -- dbus --filter "asus|rog|supergfx|power|upower"
cargo run -p rog-cli -- sensors
cargo run -p rog-cli -- caps
cargo run -p rog-cli -- privileged-status
```

The privileged helper normally runs only from packaged system D-Bus/systemd activation. For
development, build it with `cargo build -p rog-privileged`; do not run the UI or `rog-helperd` as
root. Unit tests do not require root, PolicyKit, or a live system bus.

When adding a privileged operation, add a domain-specific method and input type, an exact
internal endpoint mapping, an application-specific PolicyKit action, sanitized error mapping, and
mocked authorization/backend tests. Do not add path-, command-, program-, or shell-shaped APIs.

## Debugging Tips

### When the UI does not show expected data

Check:

- is the daemon running?
- is the daemon visible on the session bus?
- does `rog-cli` see the expected services and sensors?

Useful commands:

```bash
busctl --user list | rg -n "io\\.github\\.roghelper\\.Daemon"
cargo run -p rog-cli -- services
cargo run -p rog-cli -- caps
```

### When a control is visible but read-only

This often means:

- the feature is detected
- the current user cannot perform the write
- for CPU controls specifically, inspect the structured `control_access` data instead of relying only on `policy_writable`

Relevant areas:

- CPU writes: `crates/rog-providers/src/cpu.rs`
- keyboard backlight writes: `crates/rog-providers/src/kbd_backlight.rs`
- system DBus service availability: `crates/rog-providers/src/asusd.rs` and `crates/rog-providers/src/supergfx.rs`

### When battery, profile, or GPU controls are missing

Inspect:

- `cargo run -p rog-cli -- services`
- `cargo run -p rog-cli -- dbus --filter "asus|rog|supergfx|power|upower"`
- `cargo run -p rog-cli -- caps`

### When DBus payloads change

Update both sides together:

- serialization in `crates/rog-daemon/src/main.rs`
- decoding in `crates/rog-ui/src/main.rs`
- API docs in `docs/DBUS_API.md`

## When Docs and Code Disagree

Prefer the source.

Start with:

- `crates/rog-daemon/src/main.rs`
- `crates/rog-ui/src/main.rs`
- `crates/rog-providers/src/lib.rs`
- `crates/rog-core/src/model.rs`

Then update the matching markdown in `docs/`.

The most common current drift pattern in this repository is older roadmap or GUI planning text lagging behind implementation.

## Current Areas That Need Careful Inspection

Some areas are modeled more broadly than they are implemented:

- fan curves
- Aura / RGB lighting hardware coverage
- policy automation
- typed DBus payloads

Before documenting or extending those areas, confirm whether you are looking at:

- a live runtime feature
- a domain-layer placeholder
- a provider trait without a backend

## Documentation Discipline

When changing user-visible behavior, update the relevant docs in the same change:

- architecture changes -> `docs/ARCHITECTURE.md`
- build or validation flow -> `docs/BUILD.md`
- DBus contract -> `docs/DBUS_API.md`
- UI changes -> `docs/GUI_SPEC.md` and `docs/UI_PAGES.md`
- provider/backend changes -> `docs/PROVIDER_MATRIX.md` and `docs/FEATURE_MATRIX.md`
- troubleshooting impact -> `docs/TROUBLESHOOTING.md`
