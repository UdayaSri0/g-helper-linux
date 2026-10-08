# Graph Report - g-helper-linux  (2026-10-08)

## Corpus Check
- 104 files · ~239,503 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 2777 nodes · 7395 edges · 121 communities (115 shown, 6 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 91 edges (avg confidence: 0.78)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `8904f667`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- String
- aura.rs
- hwmon.rs
- package-common.sh
- rog-cli/src/main.rs
- cpu.rs
- AsusdPlatformProvider
- Implementation Priority
- SharedUiState
- UI Pages
- Q: How does NVIDIA GPU telemetry flow through provider, daemon, DBus, dashboard, GPU page, and diagnostics?
- fan_widgets.rs
- power_supply.rs
- supergfx.rs
- setup.rs
- memory.rs
- DBus API
- Codex Prompt: Redesign Fans Page UI with RPM Gauges, Animated Fan Rotors, Individual Fan Cards, and Safe Control UX
- Required UI changes
- config.rs
- Self
- Troubleshooting
- Vec
- display.rs
- rog-helper
- model.rs
- Current Pages
- Planned, Missing, or Pending Validation
- README.md
- Development
- DeviceCaps
- privileged.rs
- Build
- fetch_state
- Permissions
- rog-helper v0.2.2
- policy.rs
- nvidia_smi.rs
- Provider Matrix
- Validation Record
- Codex Prompt: Prepare rog-helper v0.2.2 Release
- Architecture
- Release Checklist
- Copilot Instructions for rog-helper
- KbdBacklightSysfs
- Hardware Support
- Codex Prompt: Add Safe Fan Control Page, Individual Fan Controls, Sync Mode, Manual Boost, and Fan Curve Settings to g-helper-linux
- 10. Manual control modes
- 12. Fans page layout
- PrivilegedService
- rog-helper v0.2.0
- Documentation updates
- Codex Prompt: Swap CPU and GPU Gauge Position on Fans Page
- Flatpak Packaging Notes
- Quick Start
- parse_args
- Provider implementation
- Testing requirements
- Fan-control backend discovery
- Unreleased
- rog-helper v0.2.1
- rog-helper-apprun-hook.sh
- Arch Packaging Notes
- build-flatpak.sh
- Feature design
- AGENTS.md
- postinst
- postrm
- MetricCard
- rog-privileged/src/main.rs
- build_shell
- aura_hid.rs
- SetupStatus
- Lighting
- About
- Battery
- Dashboard
- CPU
- GPU
- Memory
- Diagnostics
- Settings
- test-deb-lifecycle.py
- RogHelperDaemon
- Q: Implement robust persistent configuration and a dedicated Settings page
- String
- PrivilegedError
- dbus_decode.rs
- DependencyState
- Release Readiness Audit
- check-release-metadata.py
- DBus Contract Map
- String
- validate-package-payload.py
- rog-daemon/src/main.rs
- rog-ui/src/main.rs
- rog-helper v0.3.0
- .new
- Label
- PrivilegedStatus
- Privileged Architecture Security Review
- RogResult
- .default
- G-Helper Reference and Linux Baseline Audit
- ASUS Aura / RGB Backend Discovery
- update_diagnostics_buffer
- Option
- rog-helper v0.3.1
- v0.3.1 Release Readiness
- test-debian-maintainer-scripts.sh
- Display and Power Convenience Discovery
- test-install-dev.sh
- check-deb-package.sh
- install-dev.sh
- draw_keyboard_lighting_preview
- .set_aura_effect
- .new

## God Nodes (most connected - your core abstractions)
1. `build_ui()` - 150 edges
2. `RogHelperDaemon` - 82 edges
3. `PrivilegedService` - 43 edges
4. `FanInfo` - 38 edges
5. `AuraProvider` - 37 edges
6. `CpuCaps` - 36 edges
7. `spawn_background()` - 36 edges
8. `SharedUiState` - 35 edges
9. `HwmonTelemetryProvider` - 33 edges
10. `KbdBacklightSysfs` - 31 edges

## Surprising Connections (you probably didn't know these)
- `probe_privileged_installation()` --calls--> `scan_native_aura_hid()`  [INFERRED]
  crates/rog-cli/src/main.rs → crates/rog-providers/src/aura_hid.rs
- `cmd_setup_check()` --calls--> `probe_setup_status()`  [INFERRED]
  crates/rog-cli/src/main.rs → crates/rog-providers/src/setup.rs
- `cmd_hardware_report()` --calls--> `probe_setup_status()`  [INFERRED]
  crates/rog-cli/src/main.rs → crates/rog-providers/src/setup.rs
- `probe_lighting_diagnostics()` --calls--> `g615jm_lighting_caps()`  [INFERRED]
  crates/rog-cli/src/main.rs → crates/rog-providers/src/aura_hid.rs
- `probe_lighting_diagnostics()` --calls--> `scan_native_aura_hid()`  [INFERRED]
  crates/rog-cli/src/main.rs → crates/rog-providers/src/aura_hid.rs

## Import Cycles
- None detected.

## Communities (121 total, 6 thin omitted)

### Community 0 - "String"
Cohesion: 0.17
Nodes (36): AppState, automation_status_to_dbus(), caps_to_dbus(), caps_to_dbus_emits_feature_access_status_and_reason_keys(), caps_with_control_matrix_to_dbus(), consolidated_matrix_preserves_external_daemons_and_battery_fallback_boundary(), control_privilege_matrix(), control_privilege_row_to_dbus() (+28 more)

### Community 1 - "aura.rs"
Cohesion: 0.06
Nodes (69): AsusdAuraEffectWire, LightingMode, RgbColor, AsusdAuraContract, AsusdAuraDirection, AsusdAuraEffect, AsusdAuraSpeed, AsusdAuraZone (+61 more)

### Community 2 - "hwmon.rs"
Cohesion: 0.07
Nodes (81): FanControlRequest, FanCurve, FanCurveReadback, FanDomain, FanPoint, FanTelemetry, aggregate_fan_auto_failures(), asus_channel_paths() (+73 more)

### Community 3 - "package-common.sh"
Cohesion: 0.06
Nodes (49): append_path_dir(), download_tool(), log_section(), on_appimage_exit(), print_appdir_summary(), print_appimage_build_deps(), print_dir_listing(), print_failure_logs() (+41 more)

### Community 4 - "rog-cli/src/main.rs"
Cohesion: 0.08
Nodes (60): any_file(), backend_access_from_connect_error(), backend_connect_status_maps_error_to_temporarily_unavailable(), backend_connect_status_maps_missing_backend_without_error(), binary_on_path(), Cli, Cmd, cmd_caps() (+52 more)

### Community 5 - "cpu.rs"
Cohesion: 0.06
Nodes (77): cpu_request_readback_matches(), CpuControlRequest, CpuFrequencyBounds, CpuPowerMode, frequency_bounds_and_inversion_are_rejected(), normalize_cpu_token(), Option, RogResult (+69 more)

### Community 6 - "AsusdPlatformProvider"
Cohesion: 0.12
Nodes (30): PerformanceProfile, AsusdCaps, AsusdEndpoint, AsusdPlatformProvider, classify_dbus_error(), map_dbus_error(), map_dbus_fdo_error(), normalize_word() (+22 more)

### Community 7 - "Implementation Priority"
Cohesion: 0.10
Nodes (20): Dependency graph, Explicit blockers and non-goals, Implementation Priority, P0.1 Reconcile source truth, P0.2 Correct fan-sync capability semantics, P0.3 Validate installed privileged integration, P0.4 Supervised G615JMR Aura validation, P0.5 Supervised ASUS WMI fan validation (+12 more)

### Community 8 - "SharedUiState"
Cohesion: 0.13
Nodes (23): ComboBoxText, AutomationUiStatus, mark_ui_render_dirty(), open_uri_with_feedback(), OpenLinkRequest, PendingSavedProfileAction, persist_settings_change(), queue_saved_profile_action() (+15 more)

### Community 9 - "UI Pages"
Cohesion: 0.29
Nodes (7): Cooling, Current Planned-But-Not-Implemented Pages, Safety and discovery behavior, Setup & Access, UI Pages, What it shows, What it supports

### Community 10 - "Q: How does NVIDIA GPU telemetry flow through provider, daemon, DBus, dashboard, GPU page, and diagnostics?"
Cohesion: 0.40
Nodes (4): Answer, Outcome, Q: How does NVIDIA GPU telemetry flow through provider, daemon, DBus, dashboard, GPU page, and diagnostics?, Source Nodes

### Community 11 - "fan_widgets.rs"
Cohesion: 0.09
Nodes (38): Color, curve_plot_bounds(), curve_xy(), CurvePreview, CurvePreviewState, draw_blade(), draw_center_text(), draw_curve_preview() (+30 more)

### Community 12 - "power_supply.rs"
Cohesion: 0.09
Nodes (44): BatteryState, ambiguous_multi_battery_writes_are_rejected(), battery_dirs(), BatteryChargeLimitControl, cached_charge_limit_rejects_later_battery_hotplug(), charge_limit_bounds_are_shared_with_asusd(), charge_limit_rejects_an_attribute_symlink_escape(), charge_limit_uses_only_an_exact_battery_device() (+36 more)

### Community 13 - "supergfx.rs"
Cohesion: 0.12
Nodes (28): GpuMode, action_name_from_u32(), allows_text_mode_fallback(), classify_dbus_error(), map_dbus_error(), mode_id_from_text(), mode_name_from_text(), mode_name_from_u32() (+20 more)

### Community 14 - "setup.rs"
Cohesion: 0.11
Nodes (31): DependencyKind, DependencyStatus, best_unit_status(), binary_evidence(), bounded_probe(), build_issues(), connected_daemon_status(), cpu_permission_reports_read_only_paths() (+23 more)

### Community 15 - "memory.rs"
Cohesion: 0.15
Nodes (25): TopProcessMem, format_bytes_short(), MemorySnapshot, MemoryTelemetryProvider, parse_kib_to_bytes(), parse_meminfo_bytes(), parse_proc_status(), parse_psi_avgs() (+17 more)

### Community 16 - "DBus API"
Cohesion: 0.05
Nodes (37): Additional memory breakdown keys, Configuration, Core telemetry keys, CPU Setter Methods, DBus API, Design Notes, Errors, Fan state map (+29 more)

### Community 17 - "Codex Prompt: Redesign Fans Page UI with RPM Gauges, Animated Fan Rotors, Individual Fan Cards, and Safe Control UX"
Cohesion: 0.07
Nodes (28): 1. Top header, 2. Hero dashboard, 3. Animated fan rotor widget, 4. Circular gauge widget, 5. Individual fan cards, 6. Controls panel, 7. Fan curve preview, 8. Diagnostics section (+20 more)

### Community 18 - "Required UI changes"
Cohesion: 0.08
Nodes (25): 10. State parsing and telemetry extension, 1. Make CPU/GPU gauges bigger, 2. Put CPU and GPU gauges side by side, 3. Add operating speed / MHz display, 4. Add richer gauge card content, 5. Improve gauge drawing, 6. Reposition the Cooling Mode card, 7. Clean up fan rotor cards if needed (+17 more)

### Community 19 - "config.rs"
Cohesion: 0.09
Nodes (70): AppConfig, atomic_save_replaces_complete_file_and_cleans_temporary_file(), automation_rules_round_trip_and_reject_invalid_thresholds(), AutomationPreferences, bool_field(), CloseBehavior, config_path(), config_to_toml() (+62 more)

### Community 20 - "Self"
Cohesion: 0.08
Nodes (8): BatteryLimitPercent, CpuAuthorization, empty_fan_curve_is_rejected(), FanControlMode, FanMappingConfidence, GpuSwitchState, Into, Self

### Community 21 - "Troubleshooting"
Cohesion: 0.08
Nodes (26): asusd Present, But Fan Curves Unsupported, Battery, Profile, or GPU Controls Still Unavailable, Boost Failed or Restored Auto, `Cargo.lock` parse error (`version = 4`), CPU Counts Look Wrong On A Hybrid CPU, CPU Telemetry Works, But CPU Controls Are Read-Only, Daemon Not Running, Diagnostics Commands (+18 more)

### Community 22 - "Vec"
Cohesion: 0.33
Nodes (4): automation_skipped_components(), automation_skips_privileged_writes_and_risky_components_without_prompting(), Vec, unresolved_owned_fan_curves()

### Community 23 - "display.rs"
Cohesion: 0.18
Nodes (23): apply_refresh_rate(), apply_refresh_rate_with(), discover(), discovers_internal_output_and_rates_for_current_mode(), DisplaySnapshot, failed_mode_change_attempts_known_good_rollback(), is_internal_connector(), is_mode() (+15 more)

### Community 24 - "rog-helper"
Cohesion: 0.10
Nodes (21): AppImage Usage, Arch Linux Installation, Architecture Summary, Build, Build and Run Basics, Current Missing or Incomplete Features, Currently Implemented Features, Debian / Ubuntu / Mint Installation (+13 more)

### Community 25 - "model.rs"
Cohesion: 0.08
Nodes (18): lighting_diagnostics_explains_asusd_without_aura(), lighting_diagnostics_explains_potential_unimplemented_aura_interface(), lighting_diagnostics_formats_read_only_sysfs_warning(), lighting_diagnostics_formats_sysfs_only_brightness_report(), lighting_diagnostics_reports_redacted_native_identity_and_readiness(), lighting_request_validation_accepts_capability_gated_request(), lighting_request_validation_rejects_invalid_mode_and_missing_colour(), lighting_request_validation_rejects_irrelevant_controls() (+10 more)

### Community 26 - "Current Pages"
Cohesion: 0.09
Nodes (22): About, Battery, Cooling Page, CPU, Current Capability Behavior, Current Pages, Current Update Model, Current Window Structure (+14 more)

### Community 27 - "Planned, Missing, or Pending Validation"
Cohesion: 0.10
Nodes (21): Already Implemented, `asusd` coverage, Aura / RGB lighting, Auto mode and policy automation, Core architecture, CPU controls, Current daemon API surface, Current UI surface (+13 more)

### Community 28 - "README.md"
Cohesion: 0.18
Nodes (3): Developer Installation, Feature Matrix, Notes

### Community 29 - "Development"
Cohesion: 0.11
Nodes (19): Build and Validation Commands, Common Contributor Workflow, Current Areas That Need Careful Inspection, Debugging Tips, Development, Documentation Discipline, How the Current Crates Are Structured, Repo Layout (+11 more)

### Community 30 - "DeviceCaps"
Cohesion: 0.19
Nodes (13): DeviceCaps, combined_asusd_issue(), fan_curve_points_from_readback(), gpu_status_summary(), gpu_switch_hint_text(), lighting_privileged_access_state(), PendingFanAction, push_history() (+5 more)

### Community 31 - "privileged.rs"
Cohesion: 0.11
Nodes (15): AuthorizationState, capabilities_round_trip_and_reject_unknown_values(), denied_authorization_has_a_stable_error(), PrivilegedCapabilities, PrivilegedCategory, PrivilegedErrorCode, Into, Result (+7 more)

### Community 32 - "Build"
Cohesion: 0.11
Nodes (18): Build, Build Commands, CLI, Daemon, Install From Release Artifacts, Install Locally, Optional Desktop Launcher Install, Packaging Helpers (+10 more)

### Community 33 - "fetch_state"
Cohesion: 0.11
Nodes (39): caps_from_dbus(), caps_from_dbus_parses_feature_access_status_and_reason_keys(), caps_text_from_dbus(), cpu_caps_from_dbus(), cpu_caps_from_dbus_parses_structured_control_access_rows(), cpu_control_access_from_dbus(), cpu_path_access_from_dbus(), cpu_telemetry_from_dbus() (+31 more)

### Community 34 - "Permissions"
Cohesion: 0.09
Nodes (23): ASUS Aura RGB, ASUS profile and battery-limit control, Battery charge-limit fallback, Consolidated control / privilege matrix, CPU controls, Current Permission Boundaries, GPU mode control, Keyboard backlight brightness (+15 more)

### Community 35 - "rog-helper v0.2.2"
Cohesion: 0.12
Nodes (15): AppImage, Debian, Ubuntu, Linux Mint, Developer Notes, Direct binaries, Fedora-style RPM systems, Full Changelog, Hardware Support Notes, Highlights (+7 more)

### Community 36 - "policy.rs"
Cohesion: 0.10
Nodes (26): Cow, feature_access_reason_key(), feature_access_status_key(), String, ErrorCategory, Into, Self, String (+18 more)

### Community 37 - "nvidia_smi.rs"
Cohesion: 0.10
Nodes (35): DbusIntrospection, list_system_bus_names(), list_system_bus_names_matching(), parse_introspection(), OwnedObjectPath, RogResult, String, Vec (+27 more)

### Community 38 - "Provider Matrix"
Cohesion: 0.12
Nodes (17): `asusd`, `aura`, `aura_hid`, `cpu`, `dbus`, `hwmon`, `kbd_backlight`, `lighting` (+9 more)

### Community 39 - "Validation Record"
Cohesion: 0.12
Nodes (16): Core Controls, CPU Topology / Telemetry, Diagnostics / Capability States, Evidence, Fan Curve / Auto Safety, Fan Telemetry, Hardware Validation Template, Known Warnings (+8 more)

### Community 40 - "Codex Prompt: Prepare rog-helper v0.2.2 Release"
Cohesion: 0.15
Nodes (12): 10. Prepare Git Commands, 11. Final Report, 1. Inspect the Current Repository, 2. Create a Release Branch, 3. Update Version to v0.2.2, 4. Refresh Documentation, 5. Create Detailed Release Notes, 6. Verify Packaging (+4 more)

### Community 41 - "Architecture"
Cohesion: 0.17
Nodes (12): Architecture, Capability Model, Current Architectural Limitations, Daemon polling, DBus Contract Shape, Layered Structure, Polling Model, Provider Scope (+4 more)

### Community 42 - "Release Checklist"
Cohesion: 0.17
Nodes (12): Build, Test, and Static Checks, Cargo Metadata, Documentation, Feature Verification, Final Sign-Off, Hardware Validation Evidence, Icons and Assets, Manual Runtime Verification (+4 more)

### Community 43 - "Copilot Instructions for rog-helper"
Cohesion: 0.17
Nodes (11): Architecture & Layer Boundaries, Async & Concurrency, Common Tasks, Copilot Instructions for rog-helper, DBus & IPC, Development Workflow, Error Handling, Key Files to Know (+3 more)

### Community 44 - "KbdBacklightSysfs"
Cohesion: 0.12
Nodes (18): brightness_above_backend_maximum_is_rejected_before_io(), direct_write_is_read_back(), KbdBacklightSysfs, privileged_probe_rejects_lookalike_device(), privileged_probe_requires_exact_asus_wmi_identity_and_bounds(), privileged_write_revalidates_attribute_identity(), read_u32(), Option (+10 more)

### Community 45 - "Hardware Support"
Cohesion: 0.15
Nodes (13): Current Evidence Status, Current Unknowns, Fan-Control Validation Fields, Hardware Support, How To Add Real Evidence, Lighting Validation Fields, Minimum Evidence To Capture Per Machine, Release Scenario Matrix (+5 more)

### Community 46 - "Codex Prompt: Add Safe Fan Control Page, Individual Fan Controls, Sync Mode, Manual Boost, and Fan Curve Settings to g-helper-linux"
Cohesion: 0.20
Nodes (9): CLI diagnostics, Codex Prompt: Add Safe Fan Control Page, Individual Fan Controls, Sync Mode, Manual Boost, and Fan Curve Settings to g-helper-linux, Core goal, Final output expected from Codex, Implementation plan, Important existing architecture constraints, Linux fan-control reality, Persistent settings (+1 more)

### Community 47 - "10. Manual control modes"
Cohesion: 0.20
Nodes (10): 10. Manual control modes, 7. Add daemon fan state, 8. Add DBus methods, 9. Daemon safety behaviour, Auto / BIOS Default, Custom curve, Daemon and DBus API, Full Speed Boost (+2 more)

### Community 48 - "12. Fans page layout"
Cohesion: 0.20
Nodes (10): 11. Add a new Fans page, 12. Fans page layout, 13. UI state and error handling, Fan cards, Fan curve editor, Full speed boost section, Header/status section, Manual slider (+2 more)

### Community 49 - "PrivilegedService"
Cohesion: 0.25
Nodes (10): AsyncMutex, require_authorized(), check_polkit_authorization(), fan_domain(), PrivilegedService, Arc, Connection, Mutex (+2 more)

### Community 50 - "rog-helper v0.2.0"
Cohesion: 0.25
Nodes (7): Desktop Integration, Highlights, In-Repo Packaging Support, Known Limitations, Native Install Options, Published Release Assets, rog-helper v0.2.0

### Community 51 - "Documentation updates"
Cohesion: 0.25
Nodes (8): `docs/DBUS_API.md`, `docs/FEATURE_MATRIX.md`, `docs/GUI_SPEC.md` and `docs/UI_PAGES.md`, `docs/HARDWARE_SUPPORT.md`, `docs/PROVIDER_MATRIX.md`, `docs/TROUBLESHOOTING.md`, Documentation updates, `README.md`

### Community 52 - "Codex Prompt: Swap CPU and GPU Gauge Position on Fans Page"
Cohesion: 0.25
Nodes (7): Acceptance criteria, Codex Prompt: Swap CPU and GPU Gauge Position on Fans Page, Files to inspect, Final response expected from Codex, Required change, Responsive behaviour, Validation commands

### Community 53 - "Flatpak Packaging Notes"
Cohesion: 0.25
Nodes (7): Files, Flatpak limitations, Flatpak Packaging Notes, Future Flathub submission, Local build, Permissions, Updating cargo sources

### Community 54 - "Quick Start"
Cohesion: 0.33
Nodes (6): Developer Docs, Quick Start, Run From Source, Source File Map, User Docs, Validation

### Community 55 - "parse_args"
Cohesion: 0.53
Nodes (5): main(), parse_args(), Namespace, Path, render_icon_set()

### Community 56 - "Provider implementation"
Cohesion: 0.40
Nodes (5): 3. Extend the provider trait, 4. Extend `hwmon` fan discovery, 5. ASUS/asusd fan curve provider, 6. Backend priority, Provider implementation

### Community 57 - "Testing requirements"
Cohesion: 0.40
Nodes (5): Core validation tests, Daemon tests, Provider tests, Testing requirements, UI compile/behaviour checks

### Community 58 - "Fan-control backend discovery"
Cohesion: 0.12
Nodes (14): Aura/RGB discovery rule, DBus Notes, Fan-control discovery rule, Milestone 1, Phase 0 Discovery Commands, Rule (important), Current implementation decision, Fan-control backend discovery (+6 more)

### Community 59 - "Unreleased"
Cohesion: 0.33
Nodes (5): Added, Changed, Fixed, Safety, Unreleased

### Community 60 - "rog-helper v0.2.1"
Cohesion: 0.50
Nodes (3): Published Release Assets, Release Notes, rog-helper v0.2.1

### Community 61 - "rog-helper-apprun-hook.sh"
Cohesion: 0.50
Nodes (3): PATH, rog-helper-apprun-hook.sh script, XDG_DATA_DIRS

### Community 62 - "Arch Packaging Notes"
Cohesion: 0.50
Nodes (3): Arch Packaging Notes, Future AUR publishing, Local build from this repository

### Community 63 - "build-flatpak.sh"
Cohesion: 0.83
Nodes (3): ensure_flathub_remote(), require_flatpak_tools(), build-flatpak.sh script

### Community 64 - "Feature design"
Cohesion: 0.67
Nodes (3): 1. New shared fan domain model in `rog-core`, 2. Validation rules, Feature design

### Community 71 - "MetricCard"
Cohesion: 0.10
Nodes (21): AsRef, draw_history_graph(), HistoryGraph, HistoryGraphState, MetricCard, page_container(), page_header(), page_header_group() (+13 more)

### Community 72 - "rog-privileged/src/main.rs"
Cohesion: 0.10
Nodes (25): armed_channel_validation_rejects_a_missing_channel(), clear_fan_safety_marker_path(), fan_safety_marker_tracks_exact_semantic_channels(), HidrawDevInfo, is_safe_fan_id(), native_aura_duplicate_and_rate_limit_decisions_are_deterministic(), native_aura_request_accepts_only_high_level_verified_fields(), owned_value() (+17 more)

### Community 73 - "build_shell"
Cohesion: 0.40
Nodes (5): build_shell(), NavigationItem, Label, ViewStack, ToolbarView

### Community 75 - "aura_hid.rs"
Cohesion: 0.09
Nodes (45): AuraHidIdentity, AuraHidMatch, AuraHidProtocolFamily, AuraHidReports, AuraHidScan, breathe_packet_encodes_two_colours_and_speed(), capabilities_do_not_claim_argb_zones_or_per_key(), descriptor_parser_aggregates_multiple_output_items_only() (+37 more)

### Community 76 - "SetupStatus"
Cohesion: 0.20
Nodes (6): PermissionKind, PermissionStatus, SetupIssue, SetupSeverity, SetupStatus, fan_permission_status()

### Community 77 - "Lighting"
Cohesion: 0.33
Nodes (6): Backend behavior, Capability dependencies, Diagnostics report, Lighting, What it shows, What it supports

### Community 78 - "About"
Cohesion: 0.40
Nodes (5): About, Capability dependencies, Missing or planned, What it shows, What it supports

### Community 79 - "Battery"
Cohesion: 0.40
Nodes (5): Battery, Capability dependencies, Missing or planned, What it shows, What it supports

### Community 80 - "Dashboard"
Cohesion: 0.40
Nodes (5): Capability dependencies, Dashboard, Missing or planned, What it shows, What it supports

### Community 81 - "CPU"
Cohesion: 0.40
Nodes (5): Capability dependencies, CPU, Missing or planned, What it shows, What it supports

### Community 82 - "GPU"
Cohesion: 0.40
Nodes (5): Capability dependencies, GPU, Safety boundary, What it shows, What it supports

### Community 83 - "Memory"
Cohesion: 0.40
Nodes (5): Capability dependencies, Memory, Missing or planned, What it shows, What it supports

### Community 84 - "Diagnostics"
Cohesion: 0.40
Nodes (5): Capability dependencies, Diagnostics, Missing or planned, What it shows, What it supports

### Community 85 - "Settings"
Cohesion: 0.50
Nodes (4): Persistence and safety, Settings, What it shows, What it supports

### Community 86 - "test-deb-lifecycle.py"
Cohesion: 0.24
Nodes (22): assert_absent(), assert_exists(), assert_sentinels(), current_deb_path(), exercise(), file_manifest(), install_tree(), main() (+14 more)

### Community 87 - "RogHelperDaemon"
Cohesion: 0.09
Nodes (20): asusd_direction(), asusd_speed(), asusd_zone(), battery_automation_requires_authorization(), fan_fallback_prefers_direct_and_preserves_helper_failures(), map_privileged_lighting_error(), map_rog_error_to_fdo(), parse_gpu_mode_request() (+12 more)

### Community 88 - "Q: Implement robust persistent configuration and a dedicated Settings page"
Cohesion: 0.40
Nodes (4): Answer, Outcome, Q: Implement robust persistent configuration and a dedicated Settings page, Source Nodes

### Community 89 - "String"
Cohesion: 0.05
Nodes (102): Application, CpuCaps, CpuCoreTelemetry, CpuTelemetry, FanState, FeatureAvailability, automation_can_resume(), build_ui() (+94 more)

### Community 90 - "PrivilegedError"
Cohesion: 0.15
Nodes (27): B, PrivilegedError, apply_cpu(), battery_helper_unavailable(), call_fan(), classify_capabilities(), current_api_exposes_only_decoded_categories(), current_api_without_lighting_does_not_advertise_lighting() (+19 more)

### Community 91 - "dbus_decode.rs"
Cohesion: 0.25
Nodes (16): boolean(), float(), missing_and_wrong_type_fields_remain_absent(), nested_map(), ov(), rows(), Option, OwnedValue (+8 more)

### Community 92 - "DependencyState"
Cohesion: 0.11
Nodes (4): ContractStatus, DependencyState, FeatureAccessState, PermissionState

### Community 93 - "Release Readiness Audit"
Cohesion: 0.18
Nodes (10): Accepted v0.3.0 Limitations and Follow-Up, Build, Test, and Static Checks, Cargo Metadata, Documentation, Feature Verification, Hardware Validation Evidence and Scenarios, Icons and Assets, Manual Runtime Verification (+2 more)

### Community 94 - "check-release-metadata.py"
Cohesion: 0.57
Nodes (6): main(), parse_desktop(), png_size(), Path, require(), text()

### Community 95 - "DBus Contract Map"
Cohesion: 0.33
Nodes (6): Complete Response-Key Inventory, Consumer Summary, Contract Tests, DBus Contract Map, Default and Duplication Findings, Non-map Responses and Setter Requests

### Community 96 - "String"
Cohesion: 0.12
Nodes (17): display_backend_name(), FanCaps, LightingApplyRequest, LightingBackendKind, LightingCaps, LightingDiagnostics, LightingDirection, LightingHidDeviceDiagnostics (+9 more)

### Community 97 - "validate-package-payload.py"
Cohesion: 0.38
Nodes (13): fail(), main(), parse_control_file(), parse_service_file(), Path, Parse packaged units without resolving their staged absolute executables.…, relationship_names(), require_not_writable_by_non_root() (+5 more)

### Community 98 - "rog-daemon/src/main.rs"
Cohesion: 0.06
Nodes (48): apply_fan_caps_to_device_caps(), apply_fan_privilege_to_state(), apply_gpu_switch_status_to_caps(), apply_supergfx_probe_to_caps(), apply_supergfx_unavailable_to_caps(), authorization_denial_is_preserved_as_permission_error(), battery_helper_denial_and_unavailability_remain_distinct(), battery_state_to_str() (+40 more)

### Community 99 - "rog-ui/src/main.rs"
Cohesion: 0.05
Nodes (54): Cell, FanInfo, autostart_desktop_entry(), autostart_enabled(), autostart_entry_supports_minimized_launch(), autostart_file_path(), can_stage_update_in_dir(), compare_versions() (+46 more)

### Community 100 - "rog-helper v0.3.0"
Cohesion: 0.12
Nodes (16): AppImage, Dashboard and User Interface, Debian, Ubuntu, and Linux Mint, Developer Notes, Fedora-family systems, Full Changelog, Hardware, Telemetry, and Controls, Highlights (+8 more)

### Community 101 - ".new"
Cohesion: 0.08
Nodes (45): Client, Command, apply_battery_limit(), apply_cpu_actions(), apply_fan_action(), apply_gpu_mode(), apply_lighting(), apply_profile() (+37 more)

### Community 102 - "Label"
Cohesion: 0.08
Nodes (43): ActionRow, AccessUiState, AdministratorAccessRows, append_detail_row(), build_detail_rows(), build_fan_card_slot(), build_fan_visual_slot(), build_fans_gauge_card() (+35 more)

### Community 103 - "PrivilegedStatus"
Cohesion: 0.10
Nodes (22): LightingApplyOutcome, LightingState, PrivilegedStatus, Option, apply_cpu_privilege_to_caps(), apply_lighting_write_access(), apply_native_hid_diagnostics_selection(), automation_selects_source_rule_and_reports_missing_saved_preset() (+14 more)

### Community 104 - "Privileged Architecture Security Review"
Cohesion: 0.15
Nodes (13): 10. Compatibility, 11. Verification, 12. Change accounting, 1. Trust boundary, 2. Privileged method inventory, 3. PolicyKit actions, 4. Root-accessed resources, 5. Input and filesystem validation (+5 more)

### Community 105 - "RogResult"
Cohesion: 0.15
Nodes (14): asus_curve_policy_requires_exactly_eight_points(), conservative_presets_are_eight_point_safe_drafts(), fan_curve_sanitize_clamps(), fan_curve_sanitize_sorts_and_enforces_monotonic(), FanCurvePolicy, FanCurvePreset, parse_hex_byte(), Default (+6 more)

### Community 106 - ".default"
Cohesion: 0.18
Nodes (16): clean_programmatic_sync_does_not_create_a_draft(), cpu_policy_and_gpu_page_drafts_survive_poll_after_focus_loss(), cpu_quick_control_drafts_survive_poll_after_focus_loss(), editable_draft_failed_apply_preserves_selection_for_retry(), editable_draft_poll_never_overwrites_dirty_battery_selection(), editable_draft_reset_discards_pending_change(), editable_draft_survives_focus_loss_equivalent_and_apply_uses_draft(), editable_draft_waits_for_matching_report_before_committing_apply() (+8 more)

### Community 107 - "G-Helper Reference and Linux Baseline Audit"
Cohesion: 0.18
Nodes (11): Aura / RGB, Current Linux implementation, Documentation drift reconciled, Fans, G-Helper Reference and Linux Baseline Audit, Licensing and independent-implementation boundary, Other concepts, Pinned upstream sources (+3 more)

### Community 108 - "ASUS Aura / RGB Backend Discovery"
Cohesion: 0.25
Nodes (8): ASUS Aura / RGB Backend Discovery, Built-in effect packet comparison, Evidence, Implemented safety boundary, Provider hierarchy, Result, Upstream basis and licence decision, Validation still required

### Community 109 - "update_diagnostics_buffer"
Cohesion: 0.40
Nodes (5): Adjustment, restore_adjustment_value(), update_diagnostics_buffer(), ScrolledWindow, TextBuffer

### Community 110 - "Option"
Cohesion: 0.14
Nodes (15): automation_profile(), automation_result_state(), AutomationRuntimeStatus, ControlState, cpu_access_warning(), now_ms(), optional_lighting_string(), Default (+7 more)

### Community 111 - "rog-helper v0.3.1"
Cohesion: 0.33
Nodes (5): Known limitations, Lighting and privileged control, Release assets, Reliability and packaging, rog-helper v0.3.1

### Community 112 - "v0.3.1 Release Readiness"
Cohesion: 0.40
Nodes (5): Final sign-off, Hardware evidence and limitations, Metadata and scope, Required final-candidate gates, v0.3.1 Release Readiness

### Community 113 - "test-debian-maintainer-scripts.sh"
Cohesion: 0.60
Nodes (5): assert_has(), assert_lacks(), make_mock(), run_script(), test-debian-maintainer-scripts.sh script

### Community 114 - "Display and Power Convenience Discovery"
Cohesion: 0.40
Nodes (4): Display and Power Convenience Discovery, Findings for the inspected session, Other candidates, Validation boundary

### Community 118 - "draw_keyboard_lighting_preview"
Cohesion: 0.13
Nodes (17): Button, ColorButton, dashboard_nav_card(), draw_keyboard_lighting_preview(), lighting_quick_colour_button(), rainbow_rgba(), rgb_hex_to_rgba(), rgba_to_hex() (+9 more)

### Community 119 - ".set_aura_effect"
Cohesion: 0.24
Nodes (10): AuraWriteDecision, NativeAuraControlState, open_and_validate_aura_device(), optional_u32(), reject_if_asusd_owned(), revalidate_open_aura_fd(), Option, write_aura_report() (+2 more)

### Community 120 - ".new"
Cohesion: 0.21
Nodes (15): RogError, map_privileged_cpu_error(), map_privileged_fan_error(), HidrawReportDescriptor, main(), map_battery_error(), map_cpu_error(), map_fan_error() (+7 more)

## Knowledge Gaps
- **528 isolated node(s):** `HidrawDevInfo`, `LightingProvider`, `TelemetryProvider`, `Daemon1`, `rog-helper-apprun-hook.sh script` (+523 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **6 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Work-memory lessons

**Preferred sources** — corroborated by past sessions; start here.
- `RogHelperDaemon` (2× useful, score=1.998289323) _(code changed — re-verify)_

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `SharedUiState` connect `SharedUiState` to `String`, `fetch_state`, `rog-ui/src/main.rs`, `.new`, `PrivilegedStatus`, `.default`, `SetupStatus`, `config.rs`, `display.rs`, `String`, `DeviceCaps`?**
  _High betweenness centrality (0.035) - this node is a cross-community bridge._
- **Why does `RogHelperDaemon` connect `RogHelperDaemon` to `String`, `aura.rs`, `rog-daemon/src/main.rs`, `hwmon.rs`, `cpu.rs`, `AsusdPlatformProvider`, `PrivilegedStatus`, `aura_hid.rs`, `KbdBacklightSysfs`, `power_supply.rs`, `Option`, `supergfx.rs`, `Vec`?**
  _High betweenness centrality (0.032) - this node is a cross-community bridge._
- **Why does `PrivilegedStatus` connect `PrivilegedStatus` to `String`, `fetch_state`, `rog-daemon/src/main.rs`, `.new`, `Label`, `SharedUiState`, `String`, `PrivilegedError`, `DeviceCaps`, `privileged.rs`?**
  _High betweenness centrality (0.031) - this node is a cross-community bridge._
- **Are the 4 inferred relationships involving `build_ui()` (e.g. with `move_curve_point()` and `page_container()`) actually correct?**
  _`build_ui()` has 4 INFERRED edges - model-reasoned connections that need verification._
- **What connects `HidrawDevInfo`, `LightingProvider`, `TelemetryProvider` to the rest of the system?**
  _528 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `aura.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.05702479338842975 - nodes in this community are weakly interconnected._
- **Should `hwmon.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.06538461538461539 - nodes in this community are weakly interconnected._