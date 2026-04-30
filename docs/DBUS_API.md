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
- there is no strongly typed shared DBus schema yet

This is the current design, not a placeholder.

## Methods

| DBus method | Arguments | Response | Notes |
| --- | --- | --- | --- |
| `GetCaps` | none | `a{sv}` | Returns the top-level device capability map |
| `GetState` | none | `a{sv}` | Returns combined daemon state, including nested maps |
| `GetTelemetry` | none | `a{sv}` | Returns the current telemetry snapshot |
| `GetCpuCaps` | none | `a{sv}` | Returns CPU capability summary |
| `GetCpuTelemetry` | none | `a{sv}` | Returns CPU telemetry snapshot |
| `GetCpuDiagnostics` | none | `s` | Returns formatted text diagnostics for CPU support and state |
| `GetFanCaps` | none | `a{sv}` | Returns fan capability summary |
| `GetFanState` | none | `a{sv}` | Returns dynamic fan inventory, mode, sync, boost, and diagnostics |
| `GetFanCurves` | none | `a{sv}` | Returns fan-curve availability summary; curve reading is backend-dependent |
| `SetLighting` | `a{sv}` | `()` | Uses Aura/RGB through asusd when exposed; otherwise sysfs keyboard backlight fallback |
| `SetProfile` | `s` | `()` | Uses `asusd` when available |
| `SetGpuMode` | `s` | `()` | Uses `supergfxd` when available |
| `SetBatteryLimit` | `t` (`u64`) | `()` | Uses `asusd` when available |
| `SetCpuTurbo` | `b` | `()` | Generic CPU sysfs backend |
| `SetCpuPowerMode` | `s` | `()` | Generic CPU sysfs backend |
| `SetCpuGovernor` | `s` | `()` | Generic CPU sysfs backend |
| `SetCpuEpp` | `s` | `()` | Generic CPU sysfs backend |
| `SetCpuFreqLimits` | `tt` (`u64`, `u64`) | `()` | `0` means “leave unset / no value” for each side |
| `SetCpuCoreOnline` | `tb` (`u64`, `bool`) | `()` | Generic CPU sysfs backend |
| `SetFanAuto` | `s` | `()` | Empty string means all controllable fans |
| `SetFanManualPercent` | `st` (`string`, `u64`) | `()` | Percentage is `0..=100`; empty string or sync mode applies to all controllable fans |
| `SetFanRpmTarget` | `st` (`string`, `u64`) | `()` | Only available when writable `fanN_target` is explicitly detected |
| `SetFanCurve` | `sa{sv}` | `()` | Validates conservative temperature/speed points before provider write |
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

### Lighting map

When present, `lighting` includes:

- `backend` -> `s`, for example `asusd-aura` or `sysfs-led`
- `device` -> `s`, for example `aura-dbus:<service>:<path>:<interface>` detail or the sysfs LED name
- `brightness` -> `t`
- `max_brightness` -> `t`
- `mode` -> `s`, when reported
- `supported_modes` -> `as`
- `supports_rgb` -> `b`
- `rgb_hex` -> optional `s` in `#RRGGBB` form
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
interfaces found through introspection, RGB-looking methods/properties, fallback reasons, and
recommended next actions.

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

Each fan map includes:

- `id`, `index`, `label`
- optional `current_rpm`, `min_rpm`, `max_rpm`, `current_percent`
- `controllable`
- `supports_manual_percent`
- `supports_manual_rpm_target`
- `supports_curve`
- `supports_auto`
- `backend`
- `endpoints`, `notes`, `warnings`

Fan control methods return `InvalidArgs` for unsafe input, `NotSupported` for missing backend support, and `AccessDenied` for permission-denied sysfs writes.

## `GetTelemetry` Response

`GetTelemetry` returns the current `TelemetrySnapshot` encoded as a string-keyed `a{sv}` map.

Current telemetry keys are grouped below.

### Core telemetry keys

- `timestamp_ms`
- `cpu_temp_c`
- `gpu_temp_c`
- `gpu_core_clock_mhz`
- `gpu_memory_clock_mhz`
- `temps_c`
- `fans_rpm`
- `fan_rows`

`gpu_core_clock_mhz` and `gpu_memory_clock_mhz` are best-effort NVIDIA clock values from the daemon-side `nvidia-smi` provider. They are omitted when `nvidia-smi` is missing, unsupported, timed out, or the GPU is powered down.

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

`policy_writable` is a coarse any-writable summary. The release-grade source of truth for CPU write availability is `control_access`.

Count semantics:

- `cpu_count` is the detected physical-core count
- `thread_count` is the detected logical CPU / thread count

`control_access` is currently a list of row maps with:

- `kind` -> `boost` | `power_mode` | `governor` | `epp` | `freq_limits` | `core_online`
- `status` -> `available` | `unsupported` | `missing_backend` | `permission_denied` | `temporarily_unavailable`
- `reason` -> string
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

Current behavior:

- if Aura/RGB is exposed by asusd and `mode` or `rgb_hex` is supplied, the daemon routes that request through the Aura provider
- if Aura brightness is not exposed, brightness can still use the sysfs keyboard backlight fallback when present
- without Aura, the sysfs backend accepts brightness plus `Off` and `Static` only
- `rgb_hex` without writable Aura/RGB support returns a clear `NotSupported` error

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

Important note:

- the domain model, UI, and `asusd` backend do not currently use exactly the same default range assumptions

## CPU Setter Methods

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
- unexpected -> `Failed`

## Not Currently Exposed

The following features are not part of the current daemon API, even though older design docs mentioned them:

- fan curve setters/getters
- auto mode toggle
- auto rules setter
- diagnostics bundle export method

If those are added later, update this file at the same time as `crates/rog-daemon/src/main.rs`.
