# Release Checklist

Use this checklist before calling the repository release-ready.

## Documentation

- [ ] `README.md` reflects the current implementation and does not overstate hardware support
- [ ] `docs/ARCHITECTURE.md` matches the real runtime architecture
- [ ] `docs/BUILD.md` matches the current build and run flow
- [ ] `docs/ROADMAP.md` does not describe already implemented features as future work
- [ ] `docs/GUI_SPEC.md` and `docs/UI_PAGES.md` match the real UI
- [ ] `docs/DBUS_API.md` matches the current daemon API
- [ ] `docs/PROVIDER_MATRIX.md` matches the current provider layer
- [ ] `docs/PERMISSIONS.md` matches the current trust and write-access boundaries
- [ ] `docs/TROUBLESHOOTING.md` covers the current failure modes with practical steps
- [ ] `docs/HARDWARE_SUPPORT.md` honestly distinguishes recorded evidence from untested scenarios
- [ ] `docs/HARDWARE_VALIDATION_TEMPLATE.md` is still suitable for new machine records

## Cargo Metadata

- [ ] workspace and crate metadata reviewed
- [ ] package names are correct
- [ ] version numbers are correct
- [ ] license metadata is correct
- [ ] repository metadata reviewed
- [ ] author / maintainer metadata reviewed

Current repository note:

- the About page now reads version, license, authors, and repository URL from Cargo metadata, and derives maintainer/source metadata from the repository URL when possible, so manifest drift will show up directly in the UI

## Icons and Assets

- [ ] desktop entry reviewed
- [ ] icon names reviewed
- [ ] actual icon assets reviewed
- [ ] any About-page source metadata reviewed

Current repository note:

- the desktop entry and scalable SVG icon now live under `packaging/desktop/`, and release review should verify that they are installed together and resolve correctly on the target desktop environment

## Service and Packaging Files

- [ ] `packaging/systemd-user/rog-helperd.service` reviewed
- [ ] `packaging/desktop/rog-helper.desktop` reviewed
- [ ] user-service install flow tested
- [ ] `ExecStart` / `Exec` paths are valid for the release target

Current repository note:

- the user service expects `rog-helperd` on `PATH`

## Build, Test, and Static Checks

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo build --workspace`
- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`

Important note:

- CI is configured to run these checks, so release readiness should include confirming the current source state matches CI expectations

## Manual Runtime Verification

- [ ] daemon starts successfully
- [ ] UI starts successfully
- [ ] session DBus service is visible
- [ ] diagnostics CLI runs successfully
- [ ] tray behavior verified on the target desktop environment
- [ ] warning banner wording and Diagnostics page feel intentional rather than broken
- [ ] disabled quick controls still show a clear human-readable reason
- [ ] About page update/support actions verified with a working browser

## Hardware Validation Evidence

This section is only complete when evidence is committed and reviewable.

- [ ] every tested machine has a completed record based on `docs/HARDWARE_VALIDATION_TEMPLATE.md`
- [ ] every machine record includes tested date and tester
- [ ] every machine record includes model, distro, kernel, desktop environment, CPU, GPU, `asusd` version, and `supergfxd` version
- [ ] every machine record includes startup results for daemon, UI, tray, and diagnostics CLI
- [ ] every machine record includes results for profile control, GPU mode control, battery limit, keyboard backlight, CPU telemetry, CPU write controls, and fan telemetry
- [ ] command evidence is attached or summarized for `services`, `dbus`, `sensors`, `caps`, and session DBus introspection
- [ ] screenshot or text evidence is attached for relevant Dashboard, CPU, GPU, Diagnostics, and Lighting states
- [ ] `docs/HARDWARE_SUPPORT.md` lists recorded machines and does not imply evidence that has not been captured

## Scenario Coverage Review

Every row below should be marked either:

- covered by a real machine record, or
- explicitly `Untested`, or
- explicitly `Not represented in current test fleet`

- [ ] `asusd` missing flow
- [ ] `supergfxd` missing flow
- [ ] CPU readable-but-not-writable flow
- [ ] CPU writable flow
- [ ] keyboard backlight readable-but-not-writable flow
- [ ] fan count `0`
- [ ] fan count `1`
- [ ] fan count `2`
- [ ] fan count `3+`
- [ ] hybrid CPU topology with physical cores != logical threads

## Feature Verification

- [ ] telemetry dashboard verified
- [ ] CPU page telemetry verified
- [ ] CPU page shows physical-core count separately from logical-thread rows
- [ ] CPU page shows all logical CPUs in deterministic order
- [ ] CPU write controls verified where supported
- [ ] CPU page verified to avoid fake permission-repair actions when writes are blocked
- [ ] CPU Diagnostics verified for structured access states, sysfs paths, and suggested checks
- [ ] GPU mode read/write verified where supported
- [ ] ASUS profile read/write verified where supported
- [ ] battery charge-limit read/write verified where supported
- [ ] keyboard backlight brightness verified where supported
- [ ] dynamic fan telemetry verified against detected `fan_rows`
- [ ] Diagnostics page copy flow verified
- [ ] About page metadata verified
- [ ] About page support links verified
- [ ] manual update check verified
- [ ] safe update fallback verified for unsupported install methods

## Release Readiness Notes

Review and explicitly record:

- what is implemented and stable enough to advertise
- what is still evolving
- what is intentionally not part of the release

Current repository note:

- fan curves, Aura / RGB lighting, auto mode / policy integration, persistent configuration, and broader hardware support validation are not implemented end-to-end yet

## Final Sign-Off

- [ ] docs, code, and packaging reviewed together
- [ ] known limitations documented
- [ ] follow-up issues captured for remaining gaps
