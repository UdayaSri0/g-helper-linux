# Release Checklist

Use this checklist before calling the repository release-ready.

Latest evidence-based walkthrough: [RELEASE_READINESS_AUDIT.md](RELEASE_READINESS_AUDIT.md).
Unchecked items remain unverified even when the audit records partial automated coverage.

Stable release decision: the release owner approved `v0.3.0` on 2026-08-15. Unchecked installed-runtime, cross-distro, and real-hardware items remain explicit known limitations and must not be presented as validated support.

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
- the workspace version is now the packaging source of truth for all crates and release assets

## Icons and Assets

- [ ] desktop entry reviewed
- [ ] icon names reviewed
- [ ] actual icon assets reviewed
- [ ] any About-page source metadata reviewed

Current repository note:

- `assets/logo.png` is the master logo source, and the generated hicolor icon set is produced into `packaging/desktop/icons/hicolor/`; release review should verify that those icons, the Fans tab symbolic icon under `crates/rog-ui/assets/icons/hicolor/scalable/actions/`, the desktop entry, the AppStream metadata, the `.deb`, the `.rpm`, the AppImage, and the tarball flow all agree on the expected icon names

## Service and Packaging Files

- [ ] `packaging/systemd-user/rog-helperd.service` reviewed
- [ ] `packaging/systemd-system/rog-helper-privileged.service` reviewed against `PRIVILEGED_SECURITY_REVIEW.md`
- [ ] root-only `packaging/udev/60-rog-helper-aura.rules` and helper `DeviceAllow`/`BindPaths` reviewed; confirm no user access mode/group/uaccess tag is added
- [ ] system-D-Bus activation/policy and all four PolicyKit actions reviewed
- [ ] privileged binary is root-owned, executable, and not group/world-writable in each native package
- [ ] `packaging/scripts/validate-package-payload.py` passes for the staged Debian tree and final
  `.deb`; no `@PRIVILEGED_EXEC@` or other template token remains
- [ ] `rog-helper privileged-status` confirms helper API compatibility plus udev rule, exact Aura
  descriptor/protocol, and a valid `/dev/rog-helper-aura` alias on the target
- [ ] `packaging/desktop/rog-helper.desktop` reviewed
- [ ] `packaging/dbus-session/io.github.roghelper.Daemon.service` reviewed
- [ ] `packaging/metainfo/io.github.roghelper.UI.metainfo.xml` reviewed
- [ ] `packaging/arch/PKGBUILD` reviewed
- [ ] `packaging/arch/.SRCINFO` reviewed
- [ ] `packaging/arch/rog-helper.install` reviewed
- [ ] `packaging/flatpak/io.github.roghelper.UI.yml` reviewed
- [ ] `packaging/flatpak/cargo-sources.json` reviewed
- [ ] `packaging/flatpak/flathub.json` reviewed
- [ ] `packaging/flatpak/README.md` reviewed
- [ ] `packaging/scripts/build-deb.sh` reviewed
- [ ] `packaging/scripts/build-rpm.sh` reviewed
- [ ] `packaging/scripts/build-flatpak.sh` reviewed
- [ ] `packaging/scripts/build-tarball.sh` reviewed
- [ ] `packaging/scripts/build-appimage.sh` reviewed
- [ ] `packaging/scripts/build-release-assets.sh` reviewed
- [ ] user-service install flow tested
- [ ] `ExecStart` / `Exec` paths are valid for the release target
- [ ] release artifact names and SHA256 files reviewed

Current repository note:

- `.deb` and `.rpm` builds render absolute daemon paths under `/usr/bin`, while prefix tarballs and AppImage bundles keep a `PATH`-based daemon exec for non-`/usr` installs

## Build, Test, and Static Checks

- [ ] `cargo fmt --all -- --check`
- [ ] `cargo build --workspace`
- [ ] `cargo test --workspace`
- [ ] `cargo clippy --workspace --all-targets -- -D warnings`
- [ ] `python3 packaging/scripts/check-release-metadata.py`
- [ ] Debian package builds and `dpkg-deb --contents` confirms privileged integration ownership/modes

Important note:

- CI is configured to run these checks plus package-build smoke tests, and tagged releases are built by `.github/workflows/release.yml`

## Manual Runtime Verification

- [ ] daemon starts successfully
- [ ] UI starts successfully
- [ ] session DBus service is visible
- [ ] diagnostics CLI runs successfully
- [ ] tray behavior verified on the target desktop environment
- [ ] launcher icon resolves correctly after install
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
- [ ] G615JMR native Aura RGB physically observed for every advertised mode; `accepted_no_readback` alone is not treated as success evidence
- [ ] native Aura exposes RGB only, with ARGB/zones/per-key hidden
- [ ] native Aura PolicyKit success, denial/cancellation, helper-unavailable, duplicate/rate-limit, timeout, and active-asusd suppression flows verified
- [ ] unknown ASUS PID/interface/descriptor and multiple matching devices remain diagnostic-only and produce no write
- [ ] dynamic fan telemetry verified against detected `fan_rows`
- [ ] Diagnostics page copy flow verified
- [ ] About page metadata verified
- [ ] packaged window / tray icon naming verified
- [ ] About page support links verified
- [ ] helper killed during fan operation restores Auto after systemd restart
- [ ] helper missing/unavailable does not break telemetry or UI startup
- [ ] PolicyKit success, denial, cancellation, and missing-agent flows verified
- [ ] malformed and out-of-range system-D-Bus requests are rejected without writes
- [ ] disappearing CPU/fan/lighting/battery endpoint returns failure and refreshes actual state
- [ ] manual update check verified
- [ ] safe update fallback verified for unsupported install methods

## Release Readiness Notes

Review and explicitly record:

- what is implemented and stable enough to advertise
- what is still evolving
- what is intentionally not part of the release

Current repository note:

- the exact ASUS WMI eight-point fan-curve backend is implemented, but generic manual fan duty,
  RPM-target, and boost writes remain unsupported; Aura/RGB supports only the exact asusd
  6.3.8-6.4.0 ABI and the allow-listed G615JMR target identity, whose physical behavior is
  not yet validated; ARGB/zones/per-key and broader hardware coverage are not claimed

## Final Sign-Off

- [ ] docs, code, and packaging reviewed together
- [ ] known limitations documented
- [ ] follow-up issues captured for remaining gaps
