# Graph Report - g-helper-linux  (2026-08-13)

## Corpus Check
- 75 files · ~153,498 words
- Verdict: corpus is large enough that graph structure adds value.

## Summary
- 1740 nodes · 4293 edges · 86 communities (82 shown, 4 thin omitted)
- Extraction: 99% EXTRACTED · 1% INFERRED · 0% AMBIGUOUS · INFERRED: 32 edges (avg confidence: 0.74)
- Token cost: 0 input · 0 output

## Graph Freshness
- Built from commit: `e4aca676`
- Run `git rev-parse HEAD` and compare to check if the graph is stale.
- Run `graphify update .` after code changes (no API cost).

## Community Hubs (Navigation)
- rog-daemon/src/main.rs
- aura.rs
- hwmon.rs
- package-common.sh
- rog-cli/src/main.rs
- cpu.rs
- AsusdPlatformProvider
- rog-ui/src/main.rs
- .new
- UI Pages
- String
- fan_widgets.rs
- power_supply.rs
- SupergfxProvider
- setup.rs
- memory.rs
- DBus API
- Codex Prompt: Redesign Fans Page UI with RPM Gauges, Animated Fan Rotors, Individual Fan Cards, and Safe Control UX
- Required UI changes
- config.rs
- Self
- Troubleshooting
- build_ui
- Option
- rog-helper
- model.rs
- Current Pages
- In Progress or Still Evolving
- README.md
- Development
- SharedUiState
- Option
- Build
- FanCaps
- Permissions
- rog-helper v0.2.2
- PowerSource
- nvidia_smi.rs
- Provider Matrix
- Validation Record
- Codex Prompt: Prepare rog-helper v0.2.2 Release
- Architecture
- Release Checklist
- Copilot Instructions for rog-helper
- FeatureAvailability
- Hardware Support
- Codex Prompt: Add Safe Fan Control Page, Individual Fan Controls, Sync Mode, Manual Boost, and Fan Curve Settings to g-helper-linux
- 10. Manual control modes
- 12. Fans page layout
- update_diagnostics_buffer
- rog-helper v0.2.0
- Documentation updates
- Codex Prompt: Swap CPU and GPU Gauge Position on Fans Page
- Flatpak Packaging Notes
- Quick Start
- parse_args
- Provider implementation
- Testing requirements
- DBus Notes
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
- String
- build_shell
- OwnedValue
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

## God Nodes (most connected - your core abstractions)
1. `build_ui()` - 103 edges
2. `RogHelperDaemon` - 58 edges
3. `CpuCaps` - 30 edges
4. `FanInfo` - 28 edges
5. `HwmonTelemetryProvider` - 27 edges
6. `AuraProvider` - 26 edges
7. `spawn_background()` - 24 edges
8. `FeatureAvailability` - 23 edges
9. `SharedUiState` - 23 edges
10. `Troubleshooting` - 23 edges

## Surprising Connections (you probably didn't know these)
- `cmd_setup_check()` --calls--> `probe_setup_status()`  [INFERRED]
  crates/rog-cli/src/main.rs → crates/rog-providers/src/setup.rs
- `probe_lighting_diagnostics()` --calls--> `build_lighting_diagnostics()`  [INFERRED]
  crates/rog-cli/src/main.rs → crates/rog-providers/src/lighting.rs
- `main()` --calls--> `config_path()`  [INFERRED]
  crates/rog-daemon/src/main.rs → crates/rog-core/src/config.rs
- `load_app_settings()` --calls--> `config_path()`  [INFERRED]
  crates/rog-ui/src/main.rs → crates/rog-core/src/config.rs
- `main()` --calls--> `legacy_ui_config_path()`  [INFERRED]
  crates/rog-daemon/src/main.rs → crates/rog-core/src/config.rs

## Import Cycles
- None detected.

## Communities (86 total, 4 thin omitted)

### Community 0 - "rog-daemon/src/main.rs"
Cohesion: 0.06
Nodes (62): BatteryLimitPercent, battery_state_to_str(), caps_to_dbus(), caps_to_dbus_emits_feature_access_status_and_reason_keys(), ControlState, cpu_access_warning(), cpu_caps_to_dbus(), cpu_caps_to_dbus_emits_structured_control_access_rows() (+54 more)

### Community 1 - "aura.rs"
Cohesion: 0.08
Nodes (62): LightingMode, RgbColor, attr_value(), aura_score(), AuraControls, AuraEndpoint, AuraProbeDiagnostics, AuraProbeResult (+54 more)

### Community 2 - "hwmon.rs"
Cohesion: 0.10
Nodes (43): FanControlRequest, FanCurve, FanDomain, FanTelemetry, auto_point_endpoints(), endpoint_path(), fan(), fan_index_from_input_name() (+35 more)

### Community 3 - "package-common.sh"
Cohesion: 0.07
Nodes (45): append_path_dir(), download_tool(), log_section(), on_appimage_exit(), print_appdir_summary(), print_appimage_build_deps(), print_dir_listing(), print_failure_logs() (+37 more)

### Community 4 - "rog-cli/src/main.rs"
Cohesion: 0.07
Nodes (47): backend_access_from_connect_error(), backend_connect_status_maps_error_to_temporarily_unavailable(), backend_connect_status_maps_missing_backend_without_error(), Cli, Cmd, cmd_caps(), cmd_dbus(), cmd_fan_caps() (+39 more)

### Community 5 - "cpu.rs"
Cohesion: 0.09
Nodes (51): CpuAccessState, CpuControlAccess, CpuControlKind, CpuPathAccess, BoostPath, can_read(), choose_best_token(), collect_epp_values() (+43 more)

### Community 6 - "AsusdPlatformProvider"
Cohesion: 0.08
Nodes (42): Cow, feature_access_reason_key(), feature_access_status_key(), String, RogError, Into, Self, String (+34 more)

### Community 7 - "rog-ui/src/main.rs"
Cohesion: 0.09
Nodes (40): FanInfo, can_stage_update_in_dir(), cpu_toggle_dialog_title(), Daemon1, dashboard_mode_columns(), dashboard_secondary_columns(), detect_update_capability(), fan_card_tooltip() (+32 more)

### Community 8 - ".new"
Cohesion: 0.12
Nodes (34): Client, apply_battery_limit(), apply_cpu_actions(), apply_fan_action(), apply_gpu_mode(), apply_lighting(), apply_profile(), apply_update_action() (+26 more)

### Community 9 - "UI Pages"
Cohesion: 0.29
Nodes (7): Cooling, Current Planned-But-Not-Implemented Pages, Safety and discovery behavior, Setup & Access, UI Pages, What it shows, What it supports

### Community 10 - "String"
Cohesion: 0.10
Nodes (41): AppState, CpuCaps, CpuCoreTelemetry, CpuTelemetry, DeviceCaps, FanState, Vec, TelemetrySnapshot (+33 more)

### Community 11 - "fan_widgets.rs"
Cohesion: 0.12
Nodes (29): Color, CurvePreview, CurvePreviewState, draw_blade(), draw_center_text(), draw_curve_preview(), draw_fan_rotor(), draw_label() (+21 more)

### Community 12 - "power_supply.rs"
Cohesion: 0.12
Nodes (33): BatteryState, parse_battery_state(), pick_battery_dir(), PowerSupplyBatteryStatus, PowerSupplySysfsProvider, read_battery_health_percent(), read_i64(), read_power_w() (+25 more)

### Community 13 - "SupergfxProvider"
Cohesion: 0.17
Nodes (24): GpuMode, action_name_from_u32(), map_dbus_error(), mode_id_from_text(), mode_name_from_text(), mode_name_from_u32(), mode_name_from_value(), normalize_word() (+16 more)

### Community 14 - "setup.rs"
Cohesion: 0.15
Nodes (28): DependencyStatus, best_unit_status(), binary_evidence(), bounded_probe(), build_issues(), connected_daemon_status(), dependency_from_failed_api(), find_binary() (+20 more)

### Community 15 - "memory.rs"
Cohesion: 0.15
Nodes (25): TopProcessMem, format_bytes_short(), MemorySnapshot, MemoryTelemetryProvider, parse_kib_to_bytes(), parse_meminfo_bytes(), parse_proc_status(), parse_psi_avgs() (+17 more)

### Community 16 - "DBus API"
Cohesion: 0.06
Nodes (33): Additional memory breakdown keys, Configuration, Core telemetry keys, CPU Setter Methods, DBus API, Design Notes, Errors, Fan state map (+25 more)

### Community 17 - "Codex Prompt: Redesign Fans Page UI with RPM Gauges, Animated Fan Rotors, Individual Fan Cards, and Safe Control UX"
Cohesion: 0.07
Nodes (28): 1. Top header, 2. Hero dashboard, 3. Animated fan rotor widget, 4. Circular gauge widget, 5. Individual fan cards, 6. Controls panel, 7. Fan curve preview, 8. Diagnostics section (+20 more)

### Community 18 - "Required UI changes"
Cohesion: 0.08
Nodes (25): 10. State parsing and telemetry extension, 1. Make CPU/GPU gauges bigger, 2. Put CPU and GPU gauges side by side, 3. Add operating speed / MHz display, 4. Add richer gauge card content, 5. Improve gauge drawing, 6. Reposition the Cooling Mode card, 7. Clean up fan rotor cards if needed (+17 more)

### Community 19 - "config.rs"
Cohesion: 0.13
Nodes (39): AppConfig, atomic_save_replaces_complete_file_and_cleans_temporary_file(), bool_field(), CloseBehavior, config_path(), config_to_toml(), ConfigLoad, ConfigSource (+31 more)

### Community 20 - "Self"
Cohesion: 0.15
Nodes (7): FeatureAccessState, lighting_diagnostics_explains_asusd_without_aura(), lighting_diagnostics_explains_potential_unimplemented_aura_interface(), lighting_diagnostics_formats_read_only_sysfs_warning(), lighting_diagnostics_formats_sysfs_only_brightness_report(), Into, Self

### Community 21 - "Troubleshooting"
Cohesion: 0.09
Nodes (23): asusd Present, But Fan Curves Unsupported, Battery, Profile, or GPU Controls Still Unavailable, Boost Failed or Restored Auto, `Cargo.lock` parse error (`version = 4`), CPU Counts Look Wrong On A Hybrid CPU, CPU Telemetry Works, But CPU Controls Are Read-Only, Daemon Not Running, Diagnostics Commands (+15 more)

### Community 22 - "build_ui"
Cohesion: 0.09
Nodes (44): ActionRow, Application, Button, ComboBoxText, append_detail_row(), build_detail_rows(), build_fan_card_slot(), build_fan_visual_slot() (+36 more)

### Community 23 - "Option"
Cohesion: 0.10
Nodes (26): canonicalize_git_remote_url(), compare_versions(), detect_git_remote_repository_url(), detect_maintainer_name(), detect_repository_url(), format_bool_opt(), format_bytes_gib_opt(), format_bytes_mib_opt() (+18 more)

### Community 24 - "rog-helper"
Cohesion: 0.10
Nodes (21): AppImage Usage, Arch Linux Installation, Architecture Summary, Build, Build and Run Basics, Current Missing or Incomplete Features, Currently Implemented Features, Debian / Ubuntu / Mint Installation (+13 more)

### Community 25 - "model.rs"
Cohesion: 0.10
Nodes (17): empty_fan_curve_is_rejected(), fan_curve_sanitize_clamps(), fan_curve_sanitize_sorts_and_enforces_monotonic(), FanCurvePolicy, FanPoint, out_of_range_percent_request_is_rejected(), parse_hex_byte(), rpm_target_rejected_when_unsupported() (+9 more)

### Community 26 - "Current Pages"
Cohesion: 0.09
Nodes (22): About, Battery, Cooling Page, CPU, Current Capability Behavior, Current Pages, Current Update Model, Current Window Structure (+14 more)

### Community 27 - "In Progress or Still Evolving"
Cohesion: 0.10
Nodes (20): Already Implemented, `asusd` coverage, Aura / RGB lighting, Auto mode and policy automation, Core architecture, CPU controls, Current daemon API surface, Current UI surface (+12 more)

### Community 29 - "Development"
Cohesion: 0.11
Nodes (19): Build and Validation Commands, Common Contributor Workflow, Current Areas That Need Careful Inspection, Debugging Tips, Development, Documentation Discipline, How the Current Crates Are Structured, Repo Layout (+11 more)

### Community 30 - "SharedUiState"
Cohesion: 0.09
Nodes (24): autostart_desktop_entry(), autostart_entry_supports_minimized_launch(), initial_update_state(), open_uri_with_feedback(), OpenLinkRequest, PendingCpuAction, PendingFanAction, PendingLighting (+16 more)

### Community 31 - "Option"
Cohesion: 0.21
Nodes (4): FanControlMode, opt_text(), opt_u32(), Option

### Community 32 - "Build"
Cohesion: 0.11
Nodes (18): Build, Build Commands, CLI, Daemon, Install From Release Artifacts, Install Locally, Optional Desktop Launcher Install, Packaging Helpers (+10 more)

### Community 33 - "FanCaps"
Cohesion: 0.15
Nodes (8): FanCaps, PermissionKind, PermissionState, PermissionStatus, apply_fan_caps_to_device_caps(), cpu_permission_reports_read_only_paths(), cpu_permission_status(), fan_permission_status()

### Community 34 - "Permissions"
Cohesion: 0.11
Nodes (18): ASUS profile and battery-limit control, CPU controls, Current Permission Boundaries, GPU mode control, Keyboard backlight brightness, Permissions, Practical Guidance, Read-Only Degradation Behavior (+10 more)

### Community 35 - "rog-helper v0.2.2"
Cohesion: 0.12
Nodes (15): AppImage, Debian, Ubuntu, Linux Mint, Developer Notes, Direct binaries, Fedora-style RPM systems, Full Changelog, Hardware Support Notes, Highlights (+7 more)

### Community 36 - "PowerSource"
Cohesion: 0.26
Nodes (11): PowerSource, debounce_blocks_rapid_reapply(), manual_override_pauses_auto(), PolicyAction, PolicyConfig, PolicyEvent, PolicyState, Default (+3 more)

### Community 37 - "nvidia_smi.rs"
Cohesion: 0.19
Nodes (12): NvidiaGpuClocks, NvidiaSmiProbe, NvidiaSmiTelemetryProvider, parse_clock_mhz(), parse_clock_row(), parses_nvidia_clock_csv(), Default, Option (+4 more)

### Community 38 - "Provider Matrix"
Cohesion: 0.12
Nodes (16): `asusd`, `aura`, `cpu`, `dbus`, `hwmon`, `kbd_backlight`, `lighting`, `memory` (+8 more)

### Community 39 - "Validation Record"
Cohesion: 0.14
Nodes (14): Core Controls, CPU Topology / Telemetry, Diagnostics / Capability States, Evidence, Fan Telemetry, Hardware Validation Template, Known Warnings, Machine (+6 more)

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

### Community 44 - "FeatureAvailability"
Cohesion: 0.20
Nodes (17): FeatureAvailability, dashboard_control_subtitle(), feature_control_subtitle(), feature_state_value(), feature_status_label(), feature_value_label(), lighting_apply_subtitle(), lighting_availability_label() (+9 more)

### Community 45 - "Hardware Support"
Cohesion: 0.20
Nodes (10): Current Evidence Status, Current Unknowns, Fan-Control Validation Fields, Hardware Support, How To Add Real Evidence, Minimum Evidence To Capture Per Machine, Release Scenario Matrix, Suggested Record Naming (+2 more)

### Community 46 - "Codex Prompt: Add Safe Fan Control Page, Individual Fan Controls, Sync Mode, Manual Boost, and Fan Curve Settings to g-helper-linux"
Cohesion: 0.20
Nodes (9): CLI diagnostics, Codex Prompt: Add Safe Fan Control Page, Individual Fan Controls, Sync Mode, Manual Boost, and Fan Curve Settings to g-helper-linux, Core goal, Final output expected from Codex, Implementation plan, Important existing architecture constraints, Linux fan-control reality, Persistent settings (+1 more)

### Community 47 - "10. Manual control modes"
Cohesion: 0.20
Nodes (10): 10. Manual control modes, 7. Add daemon fan state, 8. Add DBus methods, 9. Daemon safety behaviour, Auto / BIOS Default, Custom curve, Daemon and DBus API, Full Speed Boost (+2 more)

### Community 48 - "12. Fans page layout"
Cohesion: 0.20
Nodes (10): 11. Add a new Fans page, 12. Fans page layout, 13. UI state and error handling, Fan cards, Fan curve editor, Full speed boost section, Header/status section, Manual slider (+2 more)

### Community 49 - "update_diagnostics_buffer"
Cohesion: 0.40
Nodes (5): Adjustment, restore_adjustment_value(), update_diagnostics_buffer(), ScrolledWindow, TextBuffer

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
Nodes (5): Namespace, main(), parse_args(), Path, render_icon_set()

### Community 56 - "Provider implementation"
Cohesion: 0.40
Nodes (5): 3. Extend the provider trait, 4. Extend `hwmon` fan discovery, 5. ASUS/asusd fan curve provider, 6. Backend priority, Provider implementation

### Community 57 - "Testing requirements"
Cohesion: 0.40
Nodes (5): Core validation tests, Daemon tests, Provider tests, Testing requirements, UI compile/behaviour checks

### Community 58 - "DBus Notes"
Cohesion: 0.50
Nodes (4): DBus Notes, Milestone 1, Phase 0 Discovery Commands, Rule (important)

### Community 59 - "Unreleased"
Cohesion: 0.50
Nodes (3): Added, Safety, Unreleased

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

### Community 72 - "String"
Cohesion: 0.16
Nodes (10): display_backend_name(), LightingDiagnostics, LightingState, list_text(), normalize_lighting_word(), String, build_lighting_diagnostics(), recommended_action() (+2 more)

### Community 73 - "build_shell"
Cohesion: 0.40
Nodes (5): build_shell(), NavigationItem, Label, ViewStack, ToolbarView

### Community 75 - "OwnedValue"
Cohesion: 0.18
Nodes (22): caps_from_dbus(), caps_from_dbus_parses_feature_access_status_and_reason_keys(), caps_text_from_dbus(), cpu_caps_from_dbus(), cpu_caps_from_dbus_parses_structured_control_access_rows(), cpu_control_access_from_dbus(), cpu_path_access_from_dbus(), cpu_telemetry_from_dbus() (+14 more)

### Community 76 - "SetupStatus"
Cohesion: 0.12
Nodes (5): DependencyKind, DependencyState, SetupIssue, SetupSeverity, SetupStatus

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
Nodes (5): Capability dependencies, GPU, Missing or planned, What it shows, What it supports

### Community 83 - "Memory"
Cohesion: 0.40
Nodes (5): Capability dependencies, Memory, Missing or planned, What it shows, What it supports

### Community 84 - "Diagnostics"
Cohesion: 0.40
Nodes (5): Capability dependencies, Diagnostics, Missing or planned, What it shows, What it supports

### Community 85 - "Settings"
Cohesion: 0.50
Nodes (4): Persistence and safety, Settings, What it shows, What it supports

## Knowledge Gaps
- **410 isolated node(s):** `LightingProvider`, `TelemetryProvider`, `Daemon1`, `rog-helper-apprun-hook.sh script`, `PATH` (+405 more)
  These have ≤1 connection - possible missing edges or undocumented components.
- **4 thin communities (<3 nodes) omitted from report** — run `graphify query` to explore isolated nodes.

## Suggested Questions
_Questions this graph is uniquely positioned to answer:_

- **Why does `CpuCaps` connect `String` to `rog-daemon/src/main.rs`, `FanCaps`, `cpu.rs`, `String`, `OwnedValue`, `setup.rs`, `Self`, `model.rs`, `SharedUiState`, `Option`?**
  _High betweenness centrality (0.034) - this node is a cross-community bridge._
- **Why does `FanInfo` connect `rog-ui/src/main.rs` to `rog-daemon/src/main.rs`, `hwmon.rs`, `rog-cli/src/main.rs`, `String`, `String`, `OwnedValue`, `Self`, `model.rs`, `Option`?**
  _High betweenness centrality (0.032) - this node is a cross-community bridge._
- **Why does `BatteryState` connect `power_supply.rs` to `rog-daemon/src/main.rs`, `model.rs`, `String`?**
  _High betweenness centrality (0.031) - this node is a cross-community bridge._
- **Are the 3 inferred relationships involving `build_ui()` (e.g. with `page_container()` and `page_header()`) actually correct?**
  _`build_ui()` has 3 INFERRED edges - model-reasoned connections that need verification._
- **What connects `LightingProvider`, `TelemetryProvider`, `Daemon1` to the rest of the system?**
  _410 weakly-connected nodes found - possible documentation gaps or missing edges._
- **Should `rog-daemon/src/main.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.064993564993565 - nodes in this community are weakly interconnected._
- **Should `aura.rs` be split into smaller, more focused modules?**
  _Cohesion score 0.07765293383270912 - nodes in this community are weakly interconnected._