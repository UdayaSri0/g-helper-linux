# Release Checklist

Use this checklist before calling the repository release-ready.

## Documentation

- [ ] `README.md` reflects the current implementation and degraded-state behavior
- [ ] `docs/ARCHITECTURE.md` matches the real runtime architecture
- [ ] `docs/BUILD.md` matches the current build and run flow
- [ ] `docs/ROADMAP.md` does not describe already implemented features as future work
- [ ] `docs/GUI_SPEC.md` and `docs/UI_PAGES.md` match the real UI
- [ ] `docs/DBUS_API.md` matches the current daemon API
- [ ] `docs/PROVIDER_MATRIX.md` matches the current provider layer
- [ ] `docs/PERMISSIONS.md` matches the current trust and write-access boundaries
- [ ] `docs/TROUBLESHOOTING.md` covers the current failure modes with practical steps
- [ ] `docs/HARDWARE_SUPPORT.md` contains current real-machine validation notes, not placeholders only

## Cargo Metadata

- [ ] workspace and crate metadata reviewed
- [ ] package names are correct
- [ ] version numbers are correct
- [ ] license metadata is correct
- [ ] repository metadata reviewed
- [ ] author / maintainer metadata reviewed

Current repository note:

- the UI currently falls back to hardcoded values when some Cargo metadata is missing, so release review should explicitly check manifest metadata before publishing

## Icons and Assets

- [ ] desktop entry reviewed
- [ ] icon names reviewed
- [ ] actual icon assets reviewed
- [ ] any About-page source metadata reviewed

Current repository note:

- the desktop entry references `Icon=rog-helper`, but the repository does not currently include packaged icon assets under `packaging/desktop/`

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

## Hardware Validation Matrix

Every row below should be covered by at least one real machine or explicitly marked `untested` in `docs/HARDWARE_SUPPORT.md`.

- [ ] `asusd` missing flow verified
- [ ] `supergfxd` missing flow verified
- [ ] CPU readable-but-not-writable flow verified
- [ ] CPU writable flow verified
- [ ] keyboard backlight readable-but-not-writable flow verified when hardware exposes the backend
- [ ] fan count `0` behavior verified on a machine without usable fan RPM exposure
- [ ] fan count `1` behavior verified or explicitly not represented in the test fleet
- [ ] fan count `2` behavior verified or explicitly not represented in the test fleet
- [ ] fan count `3+` behavior verified or explicitly not represented in the test fleet
- [ ] hybrid CPU topology with physical cores != logical threads verified or explicitly not represented in the test fleet

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

## Evidence Collection

- [ ] `cargo run -p rog-cli -- services` captured on each validation machine
- [ ] `cargo run -p rog-cli -- dbus --filter "asus|rog|supergfx|power|upower"` captured on each validation machine
- [ ] `cargo run -p rog-cli -- sensors` captured on each validation machine
- [ ] `cargo run -p rog-cli -- caps` captured on each validation machine
- [ ] CPU and Diagnostics screenshots captured for machines with CPU permission issues
- [ ] Dashboard, GPU, and Diagnostics screenshots captured for missing-backend scenarios
- [ ] fan telemetry screenshots captured for each distinct fan-count scenario in the test fleet

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
- [ ] release notes drafted
- [ ] follow-up issues captured for remaining gaps
