# DBus Contract Map

This is the release audit of the `io.github.roghelper.Daemon1` contract. The public wire format remains the existing string-keyed `a{sv}` API. Shared constants in `rog-core::dbus_keys` cover the high-risk state, capability, telemetry, CPU, fan, setup, and lighting names; adding a typed internal model did not rename a public key.

Decoder rule: absent optional keys and keys with the wrong DBus type are treated as unavailable. Boolean capabilities default to `false`, counts default to `0`, collections default to empty, and access states default to `unknown`. A UI fallback is presentation-only and must not claim hardware support.

## Consumer Summary

| Contract | Daemon encoder | UI decoder | CLI dependency | Primary documentation |
| --- | --- | --- | --- | --- |
| `GetState` | `state_to_dbus`, plus `RogHelperDaemon::get_state` | `fetch_state` | None | `DBUS_API.md`, this file |
| `GetCaps` | `caps_to_dbus` | `caps_from_dbus`, `caps_text_from_dbus` | None; CLI probes providers directly | `DBUS_API.md` |
| `GetTelemetry` | `telemetry_to_dbus`, `fan_row_to_dbus` | `telemetry_from_dbus`, `fan_row_from_dbus` | None; `sensors` probes providers directly | `DBUS_API.md` |
| `GetCpuCaps` | `cpu_caps_to_dbus`, access/path row encoders | `cpu_caps_from_dbus`, access/path row decoders | None | `DBUS_API.md` |
| `GetCpuTelemetry` | `cpu_to_dbus` | `cpu_telemetry_from_dbus` | None | `DBUS_API.md` |
| `GetSetupStatus` | setup status/row encoders | `setup_status_from_dbus` | None; `setup-check` uses the same core models directly | `DBUS_API.md` |
| `GetFanCaps` | `fan_caps_to_dbus` | `fan_caps_from_dbus` | None | `DBUS_API.md` |
| `GetFanState` | `fan_state_to_dbus`, `fan_info_to_dbus` | `fan_state_from_dbus`, `fan_info_from_dbus` | None | `DBUS_API.md` |
| `GetFanCurves` | inline map in `get_fan_curves` | No current UI decoder | None | `DBUS_API.md` |
| Lighting nested state | `lighting_state_to_dbus` | `lighting_from_dbus` | None; CLI probes providers directly | `DBUS_API.md` |
| Configuration and CPU diagnostics | normalized TOML / plain string | DBus proxy call or UI-local presentation | None | `DBUS_API.md`, `CONFIGURATION.md` |

The CLI intentionally has no dependency on daemon `a{sv}` response keys. Its `services`, `sensors`, `fans`, `caps`, `lighting-diagnostics`, `setup-check`, and `hardware-report` commands are read-only provider/setup probes that remain useful when the session daemon contract is unavailable.

## Complete Response-Key Inventory

Types below use DBus notation. Unless marked required, a telemetry or current-state value is optional and omitted when unavailable.

| Payload | Keys and wire types | UI missing/default behavior |
| --- | --- | --- |
| `GetState` | `caps a{sv}`, `telemetry a{sv}`, `warnings as`, `cpu_caps a{sv}`, `cpu a{sv}`, `fan_caps a{sv}`, `fan_state a{sv}`, `lighting_diagnostics_summary s`, `lighting_diagnostics_details s`; optional `lighting a{sv}`, `profile s`, `gpu_mode s`, `battery_limit t` | Missing maps decode to safe unknown/empty models. Missing current controls remain `None`; no write capability is inferred. |
| `GetCaps` | `has_profiles b`, `has_fan_curves b`, `has_fan_reading b`, `has_charge_limit b`, `has_gpu_modes b`, `has_aura b`, `has_kbd_backlight b`, `has_fan_manual_percent b`, `has_fan_manual_rpm_target b`, `has_individual_fan_control b`, `has_fan_sync_control b`, `has_fan_boost b`, `fan_count t`, `fan_backend s`, `requires_reboot_for_gpu_switch b`, `endpoints as`, `notes as`; `profile_access_{status,reason}`, `charge_limit_access_{status,reason}`, `gpu_mode_access_{status,reason}`, `kbd_backlight_access_{status,reason}` as strings | Capabilities `false`, count `0`, strings/collections empty, access `unknown`. Status values are `available`, `unsupported`, `missing_backend`, `permission_denied`, `temporarily_unavailable`, `unknown`. |
| `GetTelemetry` core | required `timestamp_ms t`; `cpu_temp_c d`, `gpu_temp_c d`, `gpu_usage_percent d`, `gpu_vram_used_bytes t`, `gpu_vram_total_bytes t`, `gpu_core_clock_mhz t`, `gpu_memory_clock_mhz t`, `gpu_power_w d`, `gpu_index u/t`, `gpu_name s`, `gpu_uuid s`, `gpu_pci_bus_id s`, `gpu_telemetry_provider s`, `gpu_telemetry_status s`, `temps_c a{sd}`, `fans_rpm a{su}`, `fan_rows aa{sv}` | Timestamp `0` if absent; every metric stays `None`; maps/lists empty. Legacy `fans_rpm` is converted to rows only when `fan_rows` is absent. |
| `GetTelemetry` power | `power_source s`, `battery_percent d`, `ac_online b`, `battery_state s`, `battery_health_percent d`, `battery_cycle_count u/t`, `battery_charge_power_w d`, `battery_discharge_power_w d`, `battery_time_to_empty_s t`, `battery_time_to_full_s t` | Missing values stay `None`. Known power source values are `Ac` and `Battery`; an unknown future value stays absent rather than being fabricated as AC. Unknown battery state maps to the domain `Unknown` state. |
| `GetTelemetry` memory | `mem_total_bytes t`, `mem_used_bytes t`, `mem_used_percent d`, `mem_available_bytes t`, `mem_free_bytes t`, `mem_cached_bytes t`, `mem_buffers_bytes t`, `mem_shared_bytes t`, `mem_anon_bytes t`, `swap_total_bytes t`, `swap_used_bytes t`, `swap_free_bytes t`, `swap_in_pages_per_s d`, `swap_out_pages_per_s d`, `zram_total_bytes t`, `zram_used_bytes t`, `zswap_enabled b` | Missing values stay `None`. |
| `GetTelemetry` PSI/breakdown | `psi_mem_some_avg10 d`, `psi_mem_some_avg60 d`, `psi_mem_some_avg300 d`, `psi_mem_full_avg10 d`, `psi_mem_full_avg60 d`, `psi_mem_full_avg300 d`, `mem_active_bytes t`, `mem_inactive_bytes t`, `mem_dirty_bytes t`, `mem_writeback_bytes t`, `mem_slab_bytes t`, `mem_sreclaimable_bytes t`, `mem_sunreclaim_bytes t`, `mem_pagetables_bytes t`, `mem_kernelstack_bytes t`, `mem_mapped_bytes t` | Missing values stay `None`. |
| `GetTelemetry` process data | `mem_top_processes s`, `mem_top_processes_rows aa{sv}`; row keys `user s`, `pid u/t`, `rss_bytes t`, `swap_bytes t`, `name s` | Text stays absent. Malformed/incomplete structured rows are skipped; an empty valid result stays absent. |
| Telemetry fan row | required row keys `hwmon_device s`, `hwmon_path s`, `input_path s`, `display_label s`; optional `raw_label s`, `rpm u/t` | A row missing identity/display keys is skipped. RPM remains optional. |
| `GetCpuCaps` | `has_cpufreq b`, `has_epp b`, `has_boost_toggle b`, `has_package_power b`, `policy_writable b`, `has_min_freq_limit b`, `has_max_freq_limit b`, `has_governor b`, `has_core_online b`, optional `scaling_driver s`, `cpu_count t`, `thread_count t`, `governor_choices as`, `epp_choices as`, `sysfs_paths as`, `control_access aa{sv}` | Capabilities `false`, counts `0`, collections empty, driver absent. |
| CPU control-access row | `kind s`, `status s`, `reason s`, `paths aa{sv}`; path row `path s`, `readable b`, `writable b` | Rows missing a known `kind` are skipped; status defaults `unknown`; read/write default `false`. |
| `GetCpuTelemetry` | required `timestamp_ms t`; `temp_c d`, `usage_percent d`, `avg_freq_mhz d`, `package_power_w d`, `status s`, `turbo_boost_enabled b`, `governor s`, `epp s`, `min_freq_mhz t`, `max_freq_mhz t`, `per_core aa{sv}` | Metrics remain absent; per-core empty. |
| CPU core row | `logical_cpu_id t`, compatibility alias `core_id t`, `physical_core_index t`, `policy_id t`, `thread_index t`, `thread_count t`, `usage_percent d`, `current_freq_mhz t`, `min_freq_mhz t`, `max_freq_mhz t`, `online b` | `logical_cpu_id` falls back to `core_id`; rows with neither are skipped. Missing `online` defaults to `true` for compatibility with older daemons that did not emit it. |
| `GetSetupStatus` | `checked_at_ms t`, `dependencies aa{sv}`, `permissions aa{sv}`, `issues aa{sv}` | Timestamp `0`; lists empty. |
| Setup dependency row | `kind s`, `state s`, `summary s`, `required_for as`, `evidence as` | Unknown `kind` rows skipped; state `unknown`; other fields empty. |
| Setup permission row | `kind s`, `state s`, `summary s`, `paths as` | Unknown `kind` rows skipped; state `unknown`; other fields empty. |
| Setup issue row | `severity s`, `title s`, `summary s`, `guidance s` | Unknown severity uses the domain fallback; strings empty. |
| `GetFanCaps` / nested `fan_caps` | all matching `GetCaps` fan booleans/count/backend/endpoints/notes plus `fan_curve_readable b`, `fan_curve_writable b`, `fan_mapping_confidence s`, `warnings as` | Booleans `false`, count `0`, mapping `unknown`, strings/lists empty. RPM telemetry never implies write support. |
| `GetFanState` | `fan_caps a{sv}`, `fans aa{sv}`, `mode s`, `sync_enabled b`, `warnings as`; optional `last_action s`, `active_boost_fan_id s`, `active_boost_until_ms t`, `active_curve_summary s` | Empty/read-only fan model, mode `read_only`, sync `false`, optionals absent. |
| Fan info row | required `id s`, `label s`; `index t`, `mapping_confidence s`, optional `current_rpm u/t`, `min_rpm u/t`, `max_rpm u/t`, `current_percent y/u/t`, `controllable b`, `supports_manual_percent b`, `supports_manual_rpm_target b`, `supports_curve b`, `supports_auto b`, `backend s`, `endpoints as`, `notes as`, `warnings as` | Rows missing id/label skipped; booleans false, index `0`, backend `unknown`, mapping `unknown`, measurements absent. |
| `GetFanCurves` | `supported b`, `reason s` | No current UI/CLI decoder. This response remains documented for external clients. |
| Lighting map | `backend s`, `backend_kind s`, `device s`, `brightness t`, `max_brightness t`, `can_set b`, `writable b`, `direct_writable b`, `privileged_writable b`, `authorization_required b`, `authorization s`, optional `mode s`, `supported_modes as`, `supports_brightness b`, `supports_modes b`, `supports_rgb b`, `supports_argb b`, `supports_zones b`, `supports_per_key b`, optional `rgb_hex s`, `secondary_rgb_hex s`, `supports_speed b`, `supported_speeds as`, optional `speed s`, `supported_directions as`, optional `direction s`, `supported_zones as`, optional `active_zone s`, `apply_outcome s`, `status s`, optional `last_error s`, `diagnostics_summary s`, `diagnostics_details s`, `fallback_reason s`, `unavailable_reason s`, `permission_warning s` | Backend/device/status use calm unknown labels, numeric values `0`, capabilities/writable false, lists empty, optional values absent. Compatibility inference enables brightness only when an older daemon reports `max_brightness > 0`, and modes only when it reports a non-empty mode list. RGB/ARGB/zones/per-key are never inferred. |

## Non-map Responses and Setter Requests

| Method | Contract |
| --- | --- |
| `GetConfiguration`, `ResetConfiguration` | Normalized versioned TOML string. |
| `SetConfiguration` | Complete TOML string; validation failure is `InvalidArgs`; no hardware setter runs. |
| `GetCpuDiagnostics` | Human-readable string; not a machine schema. |
| `SetLighting` | Request keys `brightness`, `mode`, `rgb_hex`, `secondary_rgb_hex`, `speed`, `direction`, `zone`. Unknown keys/types are rejected; unsupported backend capabilities return `NotSupported`. Native HID requests cross the system bus only as the five high-level `SetAuraEffect` strings, never as paths or bytes. |
| `SetFanCurve` | Map key `points` containing row maps with `temp_c` and `speed_percent` (or compatibility alias `duty_percent`); malformed, out-of-range, or unsafe curves are rejected before the provider. |
| Other setters | Typed DBus arguments listed in `DBUS_API.md`; they do not depend on response-map string keys. |

## Default and Duplication Findings

- Fixed: an unknown `power_source` wire string used to decode as AC. It now remains absent.
- Preserved compatibility: missing per-core `online` means `true`, because older daemon payloads implied online rows by including them. This is the only deliberate positive missing-field default.
- Preserved compatibility: older lighting payloads can infer basic brightness/mode capability from `max_brightness` and `supported_modes`; RGB, speed, zones, and writability are never inferred.
- The daemon always emits lighting `brightness`/`max_brightness` as zero when the provider has no value. Consumers must use `supports_brightness` and `writable`, not numeric presence, as the capability source of truth.
- The CLI does not duplicate these response-key strings because it does not decode daemon payloads.
- Some low-risk leaf-row keys remain flat compatibility constants. New code should use `rog_core::dbus_keys` rather than introduce another literal.

## Contract Tests

- `rog-daemon` tests encoder keys, integer wire values, nested fan/setup/CPU rows, lighting optional capabilities, and omission of unavailable telemetry.
- `rog-ui` tests capability/access decoding, structured fan/CPU rows, lighting capability decoding, mixed unsigned integer widths, missing optional fields, and unknown power-source behavior.
- `rog-core` tests canonical status mapping and centralized error-message classification.

The complete public method descriptions and accepted setter values remain in [DBUS_API.md](DBUS_API.md).

The separate privileged system API is version 2. Its native Aura surface is exactly
`SetAuraEffect(sssss) -> b`: mode, primary colour, secondary colour, speed, and direction. No path,
zone, raw packet, report ID, or command ID crosses DBus. See `DBUS_API.md` for the external asusd
6.3.8-6.4.0 signature allow-list and the privileged method inventory.
