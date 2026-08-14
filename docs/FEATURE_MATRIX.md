# Feature Matrix

This matrix reflects the current implementation in the repository today. It is based on the code in `crates/rog-core`, `crates/rog-providers`, `crates/rog-daemon`, `crates/rog-privileged`, `crates/rog-ui`, and `crates/rog-cli`.

| Feature | Backend / provider | Read / Write | Current status | Notes / limitations |
| --- | --- | --- | --- | --- |
| Session daemon API | `rog-daemon` | Read + Write | Implemented | Session bus API exposed at `io.github.roghelper.Daemon` |
| Privileged control helper | `rog-privileged` + PolicyKit | Write | Implemented, security-reviewed | On-demand hardened system service exposes typed CPU, verified fan, canonical ASUS keyboard LED, and standard battery-threshold operations; each category has a caller-bound non-retained PolicyKit action and no method accepts paths, commands, programs, environments, or raw hardware payloads |
| Privileged helper diagnostics | `rog-daemon` + `rog-cli privileged-status` | Read | Implemented | Distinguishes missing, unreachable, incompatible, PolicyKit unavailable, authorization denied, and backend/category unsupported states without breaking telemetry |
| Diagnostics CLI | `rog-cli` + provider layer | Read | Implemented | Includes consolidated `setup-check` plus service, DBus, sensor, capability, fan, and keyboard lighting/RGB inspection |
| Setup & Access readiness | `setup` provider + daemon DBus + GTK UI | Read | Implemented | Verifies expected APIs, distinguishes missing/unreachable services from read-only/unsupported controls, and exposes advanced evidence without privilege escalation |
| Capability probing | `rog-daemon` startup + providers | Read | Implemented, partial | Fan capability keys now include reading, manual percent, RPM target, curves, sync, boost, count, and backend; writable support is still hardware/backend-dependent |
| Performance profile | `asusd` | Read + Write | Implemented | Requires `asusd`; UI exposes quick actions and GPU-page controls |
| Battery charge limit | `asusd`, then standard `power_supply` ABI | Read + Write | Implemented | asusd remains preferred; fallback requires one exact `type=Battery` device with `charge_control_end_threshold`, validates 20..=100, and uses direct write before the typed PolicyKit helper |
| GPU mode | `supergfxd` | Read + Write | Implemented | Requires `supergfxd`; current safety model is hint-based rather than a full busy-state system |
| Keyboard backlight brightness | verified asusd API, then sysfs LED, then typed helper | Read + Write | Implemented | Direct routes are preferred; privileged fallback is limited to the canonical approved ASUS keyboard LED |
| Lighting mode | verified asusd Aura API or safe brightness fallback | Read + Write | Backend-dependent | Only introspection-verified asusd controls are writable; Off/Static can use brightness semantics where supported |
| Aura / RGB lighting | verified asusd DBus API | Read + Write | Backend-dependent | No generic privileged USB/HID/root backend; unverified interfaces remain unsupported |
| Fan RPM telemetry | `hwmon` | Read | Implemented | Best-effort dynamic 0..N detection; Cooling shows bounded mapped-only RPM animation, circular temperature gauges, individual cards, and collapsed diagnostics while keeping read-only fans visible |
| Fan manual percent control | generic hwmon candidates only | Write | Unsupported | Generic PWM candidates remain diagnostic-only; writable file permissions alone never authorize a write |
| Fan RPM target | generic `hwmon` candidates only | Write | Unsupported | `fanN_target` candidates remain diagnostic-only until a separately reviewed backend contract exists |
| Fan curves | verified ASUS WMI hwmon ABI | Read + Write | Backend-dependent | Direct writes are preferred; typed helper fallback is allowed only for complete validated ASUS WMI curve endpoints |
| Sync fan control | daemon fan state | Write | Implemented when possible | Available when more than one controllable fan is detected; read-only fans remain visible |
| Boost mode | no verified backend | Write | Unsupported | Generic manual-duty writes are deliberately disabled, so boost is not advertised |
| CPU telemetry | `cpu` + `hwmon` + RAPL when available | Read | Implemented | Includes usage, temperature, clocks, package power, cached 60-second filled history/sparkline presentation, physical-core/logical-thread counts, and per-logical-CPU state |
| CPU controls | `cpu` sysfs backend + privileged fallback | Read + Write | Implemented | Direct writes are preferred; only permission-blocked supported controls fall back to the helper, with validation and readback |
| Battery and power telemetry | `UPower` + `power_supply` | Read | Implemented | Best-effort combined view; sysfs fills gaps `UPower` may not expose |
| Memory and swap telemetry | `memory` provider | Read | Implemented | Includes RAM, swap, PSI, zram, zswap, and top processes |
| NVIDIA GPU telemetry | `nvidia-smi` | Read | Implemented | One daemon-side query every three seconds provides optional utilisation, VRAM, clocks, power, identity, and temperature; hwmon temperature stays preferred and all fields degrade independently |
| Diagnostics page | `rog-ui` + daemon capability/warning data | Read | Implemented | Includes structured per-control `control_privilege_matrix`, fan mapping, CPU access, battery backend/access, GPU external-service state, and lighting diagnostics |
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
