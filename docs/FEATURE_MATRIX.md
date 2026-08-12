# Feature Matrix

This matrix reflects the current implementation in the repository today. It is based on the code in `crates/rog-core`, `crates/rog-providers`, `crates/rog-daemon`, `crates/rog-ui`, and `crates/rog-cli`.

| Feature | Backend / provider | Read / Write | Current status | Notes / limitations |
| --- | --- | --- | --- | --- |
| Session daemon API | `rog-daemon` | Read + Write | Implemented | Session bus API exposed at `io.github.roghelper.Daemon` |
| Diagnostics CLI | `rog-cli` + provider layer | Read | Implemented | Includes consolidated `setup-check` plus service, DBus, sensor, capability, fan, and keyboard lighting/RGB inspection |
| Setup & Access readiness | `setup` provider + daemon DBus + GTK UI | Read | Implemented | Verifies expected APIs, distinguishes missing/unreachable services from read-only/unsupported controls, and exposes advanced evidence without privilege escalation |
| Capability probing | `rog-daemon` startup + providers | Read | Implemented, partial | Fan capability keys now include reading, manual percent, RPM target, curves, sync, boost, count, and backend; writable support is still hardware/backend-dependent |
| Performance profile | `asusd` | Read + Write | Implemented | Requires `asusd`; UI exposes quick actions and GPU-page controls |
| Battery charge limit | `asusd` | Read + Write | Implemented | Requires `asusd`; available on Battery with a Dashboard quick control |
| GPU mode | `supergfxd` | Read + Write | Implemented | Requires `supergfxd`; current safety model is hint-based rather than a full busy-state system |
| Keyboard backlight brightness | sysfs LED backend (`kbd_backlight`) | Read + Write | Implemented | Writable only if the current user can write the LED `brightness` file |
| Lighting mode | sysfs LED backend | Read + Write | Partial | Verified support is limited to `Off` and `Static`; no Aura effect contract is currently authorized |
| Aura / RGB lighting | DBus discovery provider | Read-only discovery | Not yet implemented | Aura-looking interfaces are diagnostic-only until an exact service/path/interface/signature contract is captured and tested; see `AURA_BACKEND_DISCOVERY.md` |
| Fan RPM telemetry | `hwmon` | Read | Implemented | Best-effort dynamic 0..N detection; Cooling shows bounded mapped-only RPM animation, circular temperature gauges, individual cards, and collapsed diagnostics while keeping read-only fans visible |
| Fan manual percent control | `hwmon` PWM via daemon | Write | Implemented, hardware-dependent | Enabled only when matching `pwmN` and `pwmN_enable` are writable by `rog-helperd`; UI uses percentages and provider converts to PWM |
| Fan RPM target | `hwmon` `fanN_target` via daemon | Write | Optional/backend-dependent | Hidden/disabled unless a writable `fanN_target` endpoint is explicitly detected |
| Fan curves | asusd/hwmon capability model | Read + Write | Partial/backend-dependent | Core validation, DBus API, and UI surface exist; generic hwmon curve writes stay disabled unless a backend can prove the curve format is safe |
| Sync fan control | daemon fan state | Write | Implemented when possible | Available when more than one controllable fan is detected; read-only fans remain visible |
| Boost mode | daemon + writable manual percent backend | Write | Implemented | Time-limited full-speed boost restores Auto/BIOS mode after timeout |
| CPU telemetry | `cpu` + `hwmon` + RAPL when available | Read | Implemented | Includes usage, temperature, clocks, package power, cached 60-second filled history/sparkline presentation, physical-core/logical-thread counts, and per-logical-CPU state |
| CPU controls | `cpu` sysfs backend | Read + Write | Implemented | Write access depends on sysfs permissions and platform support; daemon reports per-control access state, blocked paths, and suggested checks |
| Battery and power telemetry | `UPower` + `power_supply` | Read | Implemented | Best-effort combined view; sysfs fills gaps `UPower` may not expose |
| Memory and swap telemetry | `memory` provider | Read | Implemented | Includes RAM, swap, PSI, zram, zswap, and top processes |
| NVIDIA GPU telemetry | `nvidia-smi` | Read | Implemented | One daemon-side query every three seconds provides optional utilisation, VRAM, clocks, power, identity, and temperature; hwmon temperature stays preferred and all fields degrade independently |
| Diagnostics page | `rog-ui` + daemon capability/warning data | Read | Implemented | Structured Services/Permissions/Sensors/Warnings overview plus a collapsed copyable raw report, fan hwmon mapping, CPU access diagnostics, and keyboard lighting/RGB diagnostics |
| About page | `rog-ui` | Read | Implemented | Leads with packaged identity/icon and version, uses Cargo metadata with fallbacks, and shows maintainer info, source/support links, and release-status text |
| Manual update check / best-effort update flow | `rog-ui` + GitHub Releases API | Read + Best-effort Write | Implemented | Manual only; never requires sudo or distro package manager access; in-place replacement is limited to matching user-local direct-binary installs and otherwise falls back to opening the latest release page |
| Tray menu | `rog-ui` + `ksni` | Read + Write | Implemented | Depends on desktop support for StatusNotifierItem / AppIndicator |
| Settings page | `rog-ui` + daemon configuration API | Read + Write | Implemented | Lifecycle, dashboard visibility/compactness, inert control preferences, automation policy, and confirmed reset |
| Persistent configuration | `rog-core` + `rog-daemon` | Read + Write | Implemented | Versioned XDG `config.toml`, legacy `ui.toml` migration, field-level fallback, unknown-field tolerance, validation, and atomic replacement |
| UI lifecycle preferences | `rog-ui` + daemon config/XDG autostart | Read + Write | Implemented | Close behavior, launch-on-login, start-minimized-to-tray, and the close-to-tray hint now live on Settings |
| Remembered hardware/control preferences | `rog-daemon` config | Read + Write | Implemented, inert | Preferred charge limit, last manual profile, and fan-sync preference are saved but never applied at boot/login |
| Auto mode / policy automation | `rog-core` policy types only | Read + Write | Missing at runtime | Policy model exists, but daemon does not currently run it |
| Persistent fan curves and automation rules | None in current runtime | Read + Write | Missing | No durable fan-curve or auto-policy runtime yet |

## Notes

- The provider and domain layers already contain abstractions for fan curves, lighting, and automation beyond what the current runtime exposes.
- The current feature surface is broader than some older milestone-era docs suggest.
- When documentation and source disagree, inspect:
  - `crates/rog-daemon/src/main.rs`
  - `crates/rog-ui/src/main.rs`
  - `crates/rog-providers/src/`
