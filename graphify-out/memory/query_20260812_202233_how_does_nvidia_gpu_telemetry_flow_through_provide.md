---
type: "architecture"
date: "2026-08-12T20:22:33.860283+00:00"
question: "How does NVIDIA GPU telemetry flow through provider, daemon, DBus, dashboard, GPU page, and diagnostics?"
contributor: "graphify"
outcome: "useful"
source_nodes: ["NvidiaSmiTelemetryProvider", "NvidiaGpuTelemetry", "TelemetrySnapshot", "RogHelperDaemon", "Dashboard", "GPU"]
---

# Q: How does NVIDIA GPU telemetry flow through provider, daemon, DBus, dashboard, GPU page, and diagnostics?

## Answer

NvidiaSmiTelemetryProvider performs one bounded multi-field query, parses all physical GPU rows, deterministically selects the lowest NVIDIA index, and returns optional metrics plus identity. RogHelperDaemon refreshes it every three seconds, caches samples between one-second ticks, preserves hwmon temperature priority, and serializes optional fields through TelemetrySnapshot DBus keys. The UI parses those keys into dashboard summaries, GPU overview cards, real-sample utilisation/temperature histories, clocks, and provider diagnostics.

## Outcome

- Signal: useful

## Source Nodes

- NvidiaSmiTelemetryProvider
- NvidiaGpuTelemetry
- TelemetrySnapshot
- RogHelperDaemon
- Dashboard
- GPU