# DBus API

This document describes the current session-DBus API exported by `rog-helperd`.

The implementation lives in `crates/rog-daemon/src/main.rs`. The UI client proxy lives in `crates/rog-ui/src/main.rs`.

## Identity

- Bus name: `io.github.roghelper.Daemon`
- Object path: `/io/github/roghelper/Daemon`
- Interface: `io.github.roghelper.Daemon1`

The Rust daemon methods are implemented as snake_case functions, but the DBus-visible methods use PascalCase names such as `GetState` and `SetProfile`.

## Design Notes

The current daemon API uses string-keyed `a{sv}` dictionaries for most responses.

Benefits:

- easy to extend with optional fields
- backward-compatible growth without custom DBus structs

Tradeoffs:

- daemon and UI must agree on string keys
- payload decoding is manual
- external clients do not get a generated strongly typed schema

To reduce drift without changing the public format, daemon encoders and UI decoders now share the
high-risk wire-key constants from `rog-core::dbus_keys`, and the UI uses lossless helpers that keep
missing or wrongly typed optional values absent. The audited daemon/UI/CLI/default mapping is in
[DBUS_CONTRACT_MAP.md](DBUS_CONTRACT_MAP.md).

This is the current design, not a placeholder.

## Methods

### Configuration

- `GetConfiguration() -> s`
  - returns the current normalized, versioned TOML configuration
- `SetConfiguration(s) -> ()`
  - validates and atomically persists a complete configuration replacement
  - updates preferences only; it never invokes a hardware setter
- `ResetConfiguration() -> s`
  - atomically replaces the canonical file with safe defaults and returns normalized TOML

The daemon is the only runtime writer of `config.toml`. The UI reads the file directly only during
early startup so start-minimized behavior is known before the DBus connection is ready.

| DBus method | Arguments | Response | Notes |
| --- | --- | --- | --- |
| `GetConfiguration` | none | `s` | Returns normalized versioned TOML |
| `SetConfiguration` | `s` TOML | none | Validates and atomically persists preferences without applying hardware controls |
| `ResetConfiguration` | none | `s` | Resets the canonical configuration and returns normalized defaults |
| `GetCaps` | none | `a{sv}` | Returns the top-level device capability map |
| `GetState` | none | `a{sv}` | Returns combined daemon state, including nested maps |
| `GetTelemetry` | none | `a{sv}` | Returns the current telemetry snapshot |
| `GetCpuCaps` | none | `a{sv}` | Returns CPU capability summary |
| `GetCpuTelemetry` | none | `a{sv}` | Returns CPU telemetry snapshot |
| `GetCpuDiagnostics` | none | `s` | Returns formatted text diagnostics for CPU support and state |
| `GetSetupStatus` | none | `a{sv}` | Runs bounded, read-only service/API and permission readiness checks |
| `GetPrivilegedStatus` | none | `a{sv}` | Bounded optional-helper, compatibility, PolicyKit, authorization, and category diagnostics |
| `GetFanCaps` | none | `a{sv}` | Returns fan capability summary |
| `GetFanState` | none | `a{sv}` | Returns dynamic fan inventory, mode, sync, boost, and diagnostics |
| `GetFanCurves` | none | `a{sv}` | Returns fan-curve availability summary; curve reading is backend-dependent |
| `SetLighting` | `a{sv}` | `()` | Uses the verified sysfs brightness/Off/Static backend; Aura/RGB remains gated on an exact tested DBus contract |
| `SetProfile` | `s` | `()` | Uses `asusd` when available |
| `SetGpuMode` | `s` | `()` | Uses `supergfxd` when available |
| `SetBatteryLimit` | `t` (`u64`) | `()` | Uses `asusd` when available |
| `SetCpuTurbo` | `b` | `()` | Generic CPU sysfs backend |
| `SetCpuPowerMode` | `s` | `()` | Generic CPU sysfs backend |
| `SetCpuGovernor` | `s` | `()` | Generic CPU sysfs backend |
| `SetCpuEpp` | `s` | `()` | Generic CPU sysfs backend |
| `SetCpuFreqLimits` | `tt` (`u64`, `u64`) | `()` | `0` means “leave unset / no value” for each side |
| `SetCpuCoreOnline` | `tb` (`u64`, `bool`) | `()` | Generic CPU sysfs backend |
| `SetFanAuto` | `s` | `()` | Reserved for a verified backend; empty string means all controllable fans |
| `SetFanManualPercent` | `st` (`string`, `u64`) | `()` | Reserved for a verified backend; generic hwmon candidates are rejected |
| `SetFanRpmTarget` | `st` (`string`, `u64`) | `()` | Reserved for a verified backend; generic `fanN_target` candidates are rejected |
| `SetFanCurve` | `sa{sv}` | `()` | Validates conservative points, then returns `NotSupported` until a verified curve backend exists |
| `SetFanSync` | `b` | `()` | Enables/disables sync mode in daemon state |
| `SetFanBoost` | `stt` (`string`, `u64`, `u64`) | `()` | Time-limited manual percent boost; empty string means all controllable fans |
| `ResetFansToAuto` | none | `()` | Best-effort restore to Auto/BIOS mode |

## `GetCaps` Response

`GetCaps` returns a top-level `a{sv}` map with the following current keys:

- `has_profiles` -> `bool`
- `has_fan_curves` -> `bool`
- `has_fan_reading` -> `bool`
- `has_fan_manual_percent` -> `bool`
- `has_fan_manual_rpm_target` -> `bool`
- `has_individual_fan_control` -> `bool`
- `has_fan_sync_control` -> `bool`
- `has_fan_boost` -> `bool`
- `fan_count` -> `t`
- `fan_backend` -> `s`
- `has_charge_limit` -> `bool`
- `has_gpu_modes` -> `bool`
- `has_aura` -> `bool`
- `has_kbd_backlight` -> `bool`
- `requires_reboot_for_gpu_switch` -> `bool`
- `profile_access_status` -> `s`
- `profile_access_reason` -> `s`
- `charge_limit_access_status` -> `s`
- `charge_limit_access_reason` -> `s`
- `gpu_mode_access_status` -> `s`
- `gpu_mode_access_reason` -> `s`
- `kbd_backlight_access_status` -> `s`
- `kbd_backlight_access_reason` -> `s`
- `endpoints` -> `as`
- `notes` -> `as`

Important note:

- `has_aura` is `true` only when the daemon finds an introspectable asusd Aura/keyboard lighting interface. `has_fan_curves` is true only when a fan-curve backend is explicitly confirmed.
- `has_fan_reading` is `true` when at least one current fan RPM reading is available. `GetTelemetry.fan_rows` may still include detected fan inputs whose current RPM is unavailable.
- The `*_access_status` fields use `available`, `unsupported`, `missing_backend`, `permission_denied`, `temporarily_unavailable`, or `unknown`.
- The paired `*_access_reason` fields are short human-readable explanations intended for UI status text and troubleshooting summaries.

## `GetState` Response

`GetState` returns a top-level `a{sv}` map that currently includes:

- `caps` -> nested `a{sv}` capability map
- `telemetry` -> nested `a{sv}` telemetry map
- `warnings` -> `as`
- `cpu_caps` -> nested `a{sv}` CPU capability map
- `cpu` -> nested `a{sv}` CPU telemetry map
- `fan_caps` -> nested `a{sv}` fan capability map
- `fan_state` -> nested `a{sv}` fan state map
- `lighting_diagnostics_summary` -> `s`
- `lighting_diagnostics_details` -> `s`

Optional keys:

- `lighting` -> nested `a{sv}` lighting map
- `profile` -> `s`
- `gpu_mode` -> `s`
- `battery_limit` -> `t`

This is the main fetch path used by the current UI.

## `GetSetupStatus` Response

`GetSetupStatus` returns structured readiness data for the Setup & Access page:

- `checked_at_ms` -> `t`
- `dependencies` -> array of maps containing `kind`, `state`, `summary`, `required_for`, and advanced `evidence`
- `permissions` -> array of maps containing `kind`, `state`, `summary`, and advanced `paths`
- `issues` -> array of maps containing `severity`, `title`, `summary`, and safe human-readable `guidance`

Dependency states are `connected`, `ready`, `not_available`, `inactive`, `unreachable`,
`not_relevant`, or `unknown`. Permission states are `writable`, `read_only`, `unsupported`,
`unavailable`, or `unknown`. Probes are bounded and unprivileged; this method performs no repair,
installation, or permission change.

### Privileged status map

`GetPrivilegedStatus` keeps all system-bus access inside `rog-helperd`. Its keys are:

- `privileged_helper_installed` -> `b`
- `privileged_helper_reachable` -> `b`
- `privileged_helper_compatible` -> `b`
- `privileged_helper_version` -> `s` (empty when unavailable)
- `polkit_available` -> `b`
- `authorization_backend` -> `s` (`polkit` or `unavailable`)
- `authorization_state` -> `s` (`authorized`, `denied`, `unavailable`, or `not_checked`)
- `privileged_categories_available` -> `as`

Missing, stopped, policy-blocked, and incompatible helpers are returned as status values rather
than daemon startup failures.

## Privileged system API

`rog-helper-privileged` exports a separate system-bus interface:

- Bus name: `io.github.roghelper.Privileged`
- Object path: `/io/github/roghelper/Privileged`
- Interface: `io.github.roghelper.Privileged1`

| DBus method | Arguments | Response | Notes |
| --- | --- | --- | --- |
| `Ping` | none | `b` | Reachability only |
| `GetVersion` | none | `s` | Package version |
| `GetCapabilities` | none | `(u, as)` | API version and implemented privileged categories; currently `cpu` |
| `CanPerform` | PolicyKit action `s` | `b` | Non-interactive diagnostic check; rejects every action outside the four-item allow-list |
| `SetCpuTurbo` | `b` | `()` | Validated turbo toggle |
| `SetCpuPowerMode` | `s` | `()` | Validated preset mapped to detected governor/EPP choices |
| `SetCpuGovernor` | `s` | `()` | Value must be in every affected policy's detected allow-list |
| `SetCpuEpp` | `s` | `()` | Value must be in every affected policy's detected allow-list |
| `SetCpuFrequencyLimits` | `tt` | `()` | MHz limits; zero means omitted; bounds and ordering are validated |
| `SetCpuCoreOnline` | `tb` | `()` | Validated logical CPU ID; boot CPU 0 cannot be offlined |

The helper does not expose generic file, sysfs, process, command, or shell methods. Write methods
use specific schemas and return stable sanitized errors: `not_authorized`,
`not_supported`, `invalid_input`, `hardware_unavailable`, `permission_denied`, `backend_failure`,
or `unexpected`.

### Lighting map

When present, `lighting` includes:

- `backend` -> `s`, for example `asusd-aura` or `sysfs-led`
- `device` -> `s`, for example `aura-dbus:<service>:<path>:<interface>` detail or the sysfs LED name
- `brightness` -> `t`
- `max_brightness` -> `t`
- `mode` -> `s`, when reported
- `supported_modes` -> `as`
- `supports_brightness` -> `b`
- `supports_modes` -> `b`
- `supports_rgb` -> `b`
- `rgb_hex` -> optional `s` in `#RRGGBB` form
- `supports_speed` -> `b`
- `supported_speeds` -> `as`
- `supported_zones` -> `as`
- `can_set` / `writable` -> `b`
- `status` -> `s`, such as `available`, `rgb_not_exposed`, `rgb_unsupported`, or `backend_error`
- `last_error` -> optional `s`
- `diagnostics_summary` -> `s`
- `diagnostics_details` -> `s`
- `fallback_reason` -> optional `s`
- `unavailable_reason` -> optional `s`
- `permission_warning` -> optional `s`

The diagnostics detail string is a copyable report headed `Keyboard Lighting / RGB Diagnostics`.
It includes the selected backend, sysfs LED paths, brightness read/write state, asusd services and
interfaces found through introspection, exact candidate brightness/mode/RGB/speed/zone method and
property signatures, whether a verified contract matched, fallback reasons, and recommended next
actions.

### Fan state map

`fan_state` includes:

- `fan_caps` -> nested capability map
- `fans` -> array of nested fan maps
- `mode` -> `s`: `auto`, `manual_percent`, `manual_rpm_target`, `curve`, `full_speed_boost`, `read_only`, or `unsupported`
- `sync_enabled` -> `b`
- `last_action` -> optional `s`
- `active_boost_fan_id` -> optional `s`
- `active_boost_until_ms` -> optional `t`
- `active_curve_summary` -> optional `s`
- `warnings` -> `as`

Its nested `fan_caps` map adds these read-only discovery keys without renaming existing keys:

- `fan_curve_readable` -> `b`: at least one supported curve layout was completely readable
- `fan_curve_writable` -> `b`: a backend-specific safe write contract is active
- `fan_mapping_confidence` -> `s`: `unknown`, `heuristic`, or `hardware_label`
- `endpoints`, `notes`, `warnings` include read-only candidate-path evidence

Each fan map includes:

- `id`, `index`, `label`, and `mapping_confidence`
- optional `current_rpm`, `min_rpm`, `max_rpm`, `current_percent`
- `controllable`
- `supports_manual_percent`
- `supports_manual_rpm_target`
- `supports_curve`
- `supports_auto`
- `backend`
- `endpoints`, `notes`, `warnings`

Fan control methods return `InvalidArgs` for unsafe input and `NotSupported` when no verified backend is active. Candidate generic hwmon files never authorize a write.

## `GetTelemetry` Response

`GetTelemetry` returns the current `TelemetrySnapshot` encoded as a string-keyed `a{sv}` map.

Current telemetry keys are grouped below.

### Core telemetry keys

- `timestamp_ms`
- `cpu_temp_c`
- `gpu_temp_c`
- `gpu_usage_percent`
- `gpu_vram_used_bytes`
- `gpu_vram_total_bytes`
- `gpu_core_clock_mhz`
- `gpu_memory_clock_mhz`
- `gpu_power_w`
- `gpu_name`
- `gpu_uuid`
- `gpu_pci_bus_id`
- `gpu_index`
- `gpu_telemetry_provider`
- `gpu_telemetry_status`
- `temps_c`
- `fans_rpm`
- `fan_rows`

The optional GPU usage, VRAM, clock, power, and identity keys are best-effort values from the daemon-side `nvidia-smi` provider. The daemon selects the lowest reported NVIDIA GPU index deterministically (using PCI bus ID and UUID as tie-breakers), refreshes it every three seconds in one query, and caches the sample between one-second daemon ticks. Unsupported or `N/A` metrics are omitted individually. `gpu_temp_c` keeps its existing key and prefers the existing hwmon reading; NVIDIA temperature fills only a missing value. `gpu_telemetry_status` explains provider availability without making NVIDIA hardware mandatory.

`fans_rpm` is a flattened convenience map keyed by chosen display label and only includes rows that currently report an RPM value.

`fan_rows` is the authoritative dynamic fan-telemetry list. It may contain zero, one, or many rows depending on the platform. Rows are emitted in a deterministic order based on detected hwmon device and sysfs input path. Each row map currently includes:

- `hwmon_device`
- `hwmon_path`
- `input_path`
- optional `raw_label`
- `display_label`
- optional `rpm`

### Power and battery keys

- `power_source`
- `battery_percent`
- `ac_online`
- `battery_state`
- `battery_health_percent`
- `battery_cycle_count`
- `battery_charge_power_w`
- `battery_discharge_power_w`
- `battery_time_to_empty_s`
- `battery_time_to_full_s`

### Memory and swap keys

- `mem_total_bytes`
- `mem_used_bytes`
- `mem_used_percent`
- `mem_available_bytes`
- `mem_free_bytes`
- `mem_cached_bytes`
- `mem_buffers_bytes`
- `mem_shared_bytes`
- `mem_anon_bytes`
- `swap_total_bytes`
- `swap_used_bytes`
- `swap_free_bytes`
- `swap_in_pages_per_s`
- `swap_out_pages_per_s`
- `zram_total_bytes`
- `zram_used_bytes`
- `zswap_enabled`

### PSI keys

- `psi_mem_some_avg10`
- `psi_mem_some_avg60`
- `psi_mem_some_avg300`
- `psi_mem_full_avg10`
- `psi_mem_full_avg60`
- `psi_mem_full_avg300`

### Additional memory breakdown keys

- `mem_active_bytes`
- `mem_inactive_bytes`
- `mem_dirty_bytes`
- `mem_writeback_bytes`
- `mem_slab_bytes`
- `mem_sreclaimable_bytes`
- `mem_sunreclaim_bytes`
- `mem_pagetables_bytes`
- `mem_kernelstack_bytes`
- `mem_mapped_bytes`

### Top-process keys

- `mem_top_processes`
- `mem_top_processes_rows`

`mem_top_processes_rows` is currently a list of row maps with:

- `user`
- `pid`
- `rss_bytes`
- `swap_bytes`
- `name`

## `GetCpuCaps` Response

`GetCpuCaps` returns a CPU capability map with these current keys:

- `has_cpufreq`
- `has_epp`
- `has_boost_toggle`
- `has_package_power`
- `policy_writable`
- `has_min_freq_limit`
- `has_max_freq_limit`
- `has_governor`
- `has_core_online`
- `scaling_driver`
- `cpu_count`
- `thread_count`
- `governor_choices`
- `epp_choices`
- `sysfs_paths`
- `control_access`

`policy_writable` is a coarse any-actionable summary. The source of truth for CPU write availability is `control_access`.

Count semantics:

- `cpu_count` is the detected physical-core count
- `thread_count` is the detected logical CPU / thread count

`control_access` is currently a list of row maps with:

- `kind` -> `boost` | `power_mode` | `governor` | `epp` | `freq_limits` | `core_online`
- `status` -> `available` | `authorization_required` | `authorization_denied` | `helper_missing` | `read_only` | `unsupported` | `missing_backend` | `permission_denied` | `temporarily_unavailable`
- `reason` -> string
- `direct_write` -> `b`
- `privileged_write` -> `b`
- `authorization` -> `not_applicable` | `not_required` | `required` | `authorized` | `denied` | `unavailable`
- `paths` -> list of row maps

Each `paths` row currently includes:

- `path`
- `readable`
- `writable`

## `GetCpuTelemetry` Response

`GetCpuTelemetry` returns the current CPU telemetry map.

Current keys:

- `timestamp_ms`
- `temp_c`
- `usage_percent`
- `avg_freq_mhz`
- `package_power_w`
- `status`
- `turbo_boost_enabled`
- `governor`
- `epp`
- `min_freq_mhz`
- `max_freq_mhz`
- `per_core`

`per_core` is currently a list of row maps with:

- `logical_cpu_id`
- `physical_core_index`
- `policy_id`
- `thread_index`
- `thread_count`
- `usage_percent`
- `current_freq_mhz`
- `min_freq_mhz`
- `max_freq_mhz`
- `online`

Compatibility note:

- the daemon still includes `core_id` as a compatibility alias for `logical_cpu_id`

## `GetCpuDiagnostics` Response

`GetCpuDiagnostics` returns a plain string containing:

- CPU capability summary
- CPU write-access summary by control
- readable/writable sysfs paths involved in each control
- suggested shell checks and restart guidance
- current CPU telemetry summary
- logical CPU / thread table

This is intended for diagnostics and copy-to-clipboard workflows rather than as a machine-friendly API.

## `SetLighting`

Arguments:

- `a{sv}` map

Current accepted keys:

- `brightness` -> integer
- `mode` -> string
- `rgb_hex` -> optional string
- `speed` and `zone` are reserved capability keys and return `NotSupported` until a verified backend contract is active

Current behavior:

- the current verified backend is sysfs keyboard brightness, with `Off` and `Static` only
- brightness must be an in-range non-negative integer; invalid RGB strings and empty modes return `InvalidArgs`
- unknown keys return `InvalidArgs`; unsupported RGB/effect/speed/zone requests return `NotSupported`
- Aura routing remains disabled until an exact introspected control contract is captured and tested
- a future verified Aura backend may still use the sysfs brightness fallback when Aura does not expose brightness

## `SetProfile`

Argument:

- string profile name

Current accepted user-facing values:

- `Silent`
- `Balanced`
- `Turbo`
- `Custom`

The daemon normalizes several textual aliases internally.

## `SetGpuMode`

Argument:

- string GPU mode name

Current accepted user-facing values:

- `Integrated`
- `Hybrid`
- `Dedicated`

The daemon also normalizes a few related aliases such as `dynamic`, `optimus`, and `discrete`.

## `SetBatteryLimit`

Argument:

- `u64`

Current behavior:

- converted to `u8` by the daemon
- applied through `asusd`
- current backend validation ultimately depends on the `asusd` provider

Range note:

- the asusd wire backend accepts its verified `20..=100` range
- the UI and persistent preferred-limit configuration deliberately expose the narrower `40..=100` policy range
- this is a product-policy restriction, not a decoding default; out-of-range requests return an error

## CPU Setter Methods

The session daemon always tries the unprivileged provider first. Only `permission_denied` falls
back to the corresponding privileged system method. Unsupported hardware and invalid input do not
cross the privilege boundary. Successful calls are refreshed and checked against telemetry when
readback is available.

### `SetCpuTurbo`

- argument: `bool`

### `SetCpuPowerMode`

- argument: string
- current backend understands quiet/silent, balanced, performance/turbo style modes

### `SetCpuGovernor`

- argument: string

### `SetCpuEpp`

- argument: string

### `SetCpuFreqLimits`

- arguments: `min_mhz`, `max_mhz` as `u64`
- daemon interpretation:
  - `0` -> no value / unset side
  - non-zero -> explicit MHz value

### `SetCpuCoreOnline`

- arguments:
  - `core_id` as `u64` logical CPU / thread id
  - `online` as `bool`

## Errors

The daemon maps its internal `RogError` categories to DBus `fdo::Error` values.

Current mapping:

- dependency missing -> `Failed`
- not supported -> `NotSupported`
- permission denied -> `AccessDenied`
- invalid input -> `InvalidArgs`
- transient failure -> `TimedOut`
- temporarily unavailable -> `Failed`
- unexpected -> `Failed`

The internal presentation taxonomy additionally distinguishes `read-only`, `missing dependency`,
`permission denied`, `transient failure`, and general `unavailable` states. This does not rename
existing DBus errors or status strings.

## Not Currently Exposed

The following features are not part of the current daemon API:

- a verified fan-curve read/write backend (the method surface exists, but writes are capability-gated off)
- auto rules setter
- diagnostics bundle export DBus method (a read-only Markdown export is available from
  `rog-helper hardware-report`)

If those are added later, update this file at the same time as `crates/rog-daemon/src/main.rs`.
