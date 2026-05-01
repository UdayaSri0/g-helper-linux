# Codex Prompt: Swap CPU and GPU Gauge Position on Fans Page

Repository: https://github.com/UdayaSri0/g-helper-linux

The Fans page UI is now looking good. I only need one small layout change.

## Required change

On the Fans page hero/dashboard section, change the order of the large temperature/speed gauge cards:

Current order:

```text
[ GPU Gauge ] [ CPU Gauge ]
```

Required order:

```text
[ CPU Gauge ] [ GPU Gauge ]
```

So:

- CPU temperature gauge must appear on the left.
- GPU temperature gauge must appear on the right.
- CPU should keep the magenta/pink accent.
- GPU should keep the blue/cyan accent.
- Do not change the gauge design, size, animation, fan cards, controls, diagnostics, or backend logic.
- Do not change telemetry parsing.
- Do not change DBus/provider/daemon code unless absolutely necessary.
- This should be a UI layout-only change.

## Files to inspect

Likely files:

```text
crates/rog-ui/src/fans_page.rs
crates/rog-ui/src/fan_widgets.rs
crates/rog-ui/src/main.rs
```

Find where the gauge row is built. It may look similar to:

```rust
gauge_row.append(&gpu_gauge_card);
gauge_row.append(&cpu_gauge_card);
```

Change it to:

```rust
gauge_row.append(&cpu_gauge_card);
gauge_row.append(&gpu_gauge_card);
```

If the layout uses a `gtk::Grid`, `gtk::FlowBox`, or helper function instead of a simple `Box`, update the child insertion/order so CPU comes first and GPU comes second.

## Responsive behaviour

Keep the same responsive behaviour:

Wide layout:

```text
[ CPU Gauge ] [ GPU Gauge ]
```

Narrow layout:

```text
[ CPU Gauge ]
[ GPU Gauge ]
```

Do not allow GPU to appear before CPU in narrow mode either.

## Acceptance criteria

After the change:

- CPU gauge is shown on the left.
- GPU gauge is shown on the right.
- CPU still shows temperature and average clock MHz.
- GPU still shows temperature, core clock, and memory clock if available.
- Fan cards remain unchanged.
- Safe Controls section remains unchanged.
- Read-only fan behaviour remains unchanged.
- No visual regression in dark theme.
- No layout clipping.

## Validation commands

Run:

```bash
cargo fmt --all -- --check
cargo build --workspace
cargo test --workspace
cargo clippy --workspace --all-targets -- -D warnings
```

Manual test:

```bash
cargo run -p rog-daemon
cargo run -p rog-ui
```

Open the Fans page and confirm the gauge order is:

```text
CPU left, GPU right
```

## Final response expected from Codex

Provide:

1. Files changed.
2. Exact place where the gauge order was swapped.
3. Confirmation that no backend/daemon/provider behaviour was changed.
4. Validation commands run and results.
