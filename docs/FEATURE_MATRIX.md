# Feature Matrix

This matrix reflects the current implementation in the repository today. It is based on the code in `crates/rog-core`, `crates/rog-providers`, `crates/rog-daemon`, `crates/rog-ui`, and `crates/rog-cli`.

| Feature | Backend / provider | Read / Write | Current status | Notes / limitations |
| --- | --- | --- | --- | --- |
| Session daemon API | `rog-daemon` | Read + Write | Implemented | Session bus API exposed at `io.github.roghelper.Daemon` |
| Diagnostics CLI | `rog-cli` + provider layer | Read | Implemented | Useful for service, DBus, sensor, and capability inspection |
| Capability probing | `rog-daemon` startup + providers | Read | Implemented, partial | `has_profiles`, `has_charge_limit`, `has_gpu_modes`, `has_fan_reading`, `has_kbd_backlight` are populated; `has_aura` and `has_fan_curves` are not currently populated to true |
| Performance profile | `asusd` | Read + Write | Implemented | Requires `asusd`; UI exposes quick actions and GPU-page controls |
| Battery charge limit | `asusd` | Read + Write | Implemented | Requires `asusd`; current UI control lives on Dashboard |
| GPU mode | `supergfxd` | Read + Write | Implemented | Requires `supergfxd`; current safety model is hint-based rather than a full busy-state system |
| Keyboard backlight brightness | sysfs LED backend (`kbd_backlight`) | Read + Write | Implemented | Writable only if the current user can write the LED `brightness` file |
| Lighting mode | sysfs LED backend | Read + Write | Partial | Current daemon backend supports `Off` and `Static` only |
| Aura / RGB lighting | None in current runtime | Read + Write | Missing | Model and UI placeholders exist, but no runtime backend is wired in |
| Fan RPM telemetry | `hwmon` | Read | Implemented | Best-effort dynamic 0..N detection; friendly labels come from hwmon when available, otherwise the UI falls back to `Fan N`; rows remain visible even when an individual input is currently unavailable |
| Fan curves | None in current runtime | Read + Write | Missing | Domain model and traits exist, but there is no provider, daemon API, or UI flow yet |
| CPU telemetry | `cpu` + `hwmon` + RAPL when available | Read | Implemented | Includes usage, temperature, clocks, package power, physical-core/logical-thread counts, and per-logical-CPU state |
| CPU controls | `cpu` sysfs backend | Read + Write | Implemented | Write access depends on sysfs permissions and platform support; daemon reports per-control access state, blocked paths, and suggested checks |
| Battery and power telemetry | `UPower` + `power_supply` | Read | Implemented | Best-effort combined view; sysfs fills gaps `UPower` may not expose |
| Memory and swap telemetry | `memory` provider | Read | Implemented | Includes RAM, swap, PSI, zram, zswap, and top processes |
| NVIDIA GPU temperature fallback | `nvidia-smi` | Read | Implemented | Used only when primary GPU temperature is unavailable from `hwmon` |
| Diagnostics page | `rog-ui` + daemon capability/warning data | Read | Implemented | Copyable text view with a troubleshooting summary, structured feature-access reasons, raw fan hwmon mapping, and CPU access diagnostics |
| About page | `rog-ui` | Read | Implemented | Uses Cargo metadata when present and fallbacks when it is missing; also shows maintainer info, source/support links, and release-status text |
| Manual update check / best-effort update flow | `rog-ui` + GitHub Releases API | Read + Best-effort Write | Implemented | Manual only; never requires sudo or distro package manager access; in-place replacement is limited to matching user-local direct-binary installs and otherwise falls back to opening the latest release page |
| Tray menu | `rog-ui` + `ksni` | Read + Write | Implemented | Depends on desktop support for StatusNotifierItem / AppIndicator |
| Auto mode / policy automation | `rog-core` policy types only | Read + Write | Missing at runtime | Policy model exists, but daemon does not currently run it |
| Persistent configuration | None in current runtime | Read + Write | Missing | No saved settings or rule persistence in current code |

## Notes

- The provider and domain layers already contain abstractions for fan curves, lighting, and automation beyond what the current runtime exposes.
- The current feature surface is broader than some older milestone-era docs suggest.
- When documentation and source disagree, inspect:
  - `crates/rog-daemon/src/main.rs`
  - `crates/rog-ui/src/main.rs`
  - `crates/rog-providers/src/`
