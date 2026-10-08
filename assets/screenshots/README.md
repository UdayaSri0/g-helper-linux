# Screenshot provenance

These five files are real pre-release application captures taken on 2026-10-08 from an ASUS ROG
test system. They are not mock-ups, physical hardware-validation records, or evidence that every
shown feature is writable on other machines. The captures intentionally preserve capability-driven
read-only and degraded states.

| File | Dimensions | Visible state |
| --- | --- | --- |
| `dashboard.png` | 1919 × 1040 | Connected daemon with telemetry available, five setup items, and read-only controls. |
| `fans.png` | 1916 × 1039 | Four RPM inputs, read-only controls, and an explicitly uncertain fan mapping. |
| `lighting.png` | 1917 × 1158 | The narrow native Aura target is detected, but the installed privileged lighting service is incompatible, so controls are unavailable. |
| `setup-access.png` | 1919 × 1159 | A pre-v0.4 Setup & Access capture that visibly reports service/UI version 0.3.1 and missing optional integrations. It is retained as diagnostic-state evidence but is not used as a v0.4 showcase image. |
| `cpu.png` | 1919 × 1158 | CPU telemetry is available while write controls are blocked. |

The README uses Dashboard, Fan Control, Lighting, and CPU Telemetry in a compact 2×2 grid. A future
recapture should use the final packaged version and a compatible helper before replacing the
Setup & Access image in that grid.

The images contain live temperatures, utilisation, clocks, fan RPM, and capability/readiness
states. They do not visibly contain a username, hostname, serial number, home path, or account
identifier. Review every replacement capture for those details before publishing it.
