# Release Readiness Audit

Audit date: 2026-08-13

This report walks [RELEASE_CHECKLIST.md](RELEASE_CHECKLIST.md) without converting unverified work into checked items. The repository is **not yet ready for final release sign-off** because installed-runtime, cross-package, and real-hardware evidence is incomplete.

## Documentation

Status: reviewed for this hardening pass, but not manually validated end-to-end against every UI page and supported hardware backend.

- `DBUS_API.md` now points to the complete encoder/decoder/default audit in `DBUS_CONTRACT_MAP.md`.
- Architecture and build docs describe the shared internal key/status model while retaining the public `a{sv}` API.
- `HARDWARE_SUPPORT.md` and its template now recommend the read-only `rog-helper hardware-report` capture.
- Hardware-support claims remain explicitly separate from real validation evidence; no machine record was committed by this audit.
- Broader provider, UI-page, roadmap, troubleshooting, and permissions documents still require release-owner/manual review as listed in the checklist.

## Cargo Metadata

Status: verified by `packaging/scripts/check-release-metadata.py`.

- Workspace version: `0.2.2`.
- All crates inherit workspace version, edition, authors, homepage, license, and repository.
- License: `MIT OR Apache-2.0`; both license files are present and packaged.
- Homepage/repository: `https://github.com/UdayaSri0/g-helper-linux`.
- Binary names agree across Cargo and AppStream: `rog-helper`, `rog-helperd`, `rog-helper-ui`.
- Maintainer metadata is present. Final release ownership/maintainer approval remains a human sign-off item.

## Icons and Assets

Status: source and packaged assets verified; installed desktop rendering remains unverified.

- The metadata checker validates the master logo, generated 16/24/32/48/64/128/256/512 PNG dimensions, desktop icon name, and 11 compiled GTK resources.
- UI/About/header symbolic assets are compiled into `rog-helper-ui`; the fan symbolic icon is also installed in the icon theme.
- Debian and tarball content inspection confirmed the application icons, pixmap, desktop entry, and AppStream metadata are present.
- Launcher, window, and tray icon rendering after a real install remains unverified.

## Service and Packaging Files

Status: native Debian and prefix-tarball smoke tests pass; other formats are blocked by missing host tools.

- Desktop, AppStream, session DBus, systemd user unit, Arch metadata, Flatpak manifest files, Debian templates, RPM template, and build scripts were statically audited.
- `desktop-file-validate` and `appstreamcli validate --no-net` pass.
- A release workspace build, Debian package, prefix tarball, direct binaries, and SHA256 manifest were produced in a temporary directory.
- `sha256sum -c` passed for all generated native artifacts.
- Debian/tarball content inspection confirmed binaries, desktop/AppStream metadata, DBus activation, user service, icons, and licenses.
- Debian service paths resolve to `/usr/bin/rog-helperd`; prefix-tarball services use `rog-helperd` with `/usr/local/bin` in the unit PATH.
- Generated DBus/systemd files are now forced to mode `0644`; the previous renderer inherited a group-writable umask.
- Debian dependency generation emitted usr-merge diversion and staging-layout warnings, but completed and produced explicit GTK/libadwaita/glibc dependencies. Validate on a clean package-build container before sign-off.
- Flatpak validation is blocked on this host by missing `flatpak-builder` and `jq`.
- AppImage validation is blocked by missing `patchelf`, `gobject-introspection-1.0`, and `librsvg-2.0` development metadata.
- RPM build is blocked by missing `rpmbuild`/`rpm`; Arch verification is blocked by missing `makepkg`.
- Package installation, upgrade, uninstall, DBus activation, and user-service enablement were not performed.

## Build, Test, and Static Checks

Status: pass for the source tree at the end of this audit.

- `cargo fmt --all -- --check`: pass.
- `cargo clippy --workspace --all-targets -- -D warnings`: pass.
- `cargo test --workspace`: pass; 86 tests, 0 failures.
- `cargo build --workspace`: pass.
- `git diff --check`: pass.
- Packaging metadata/resource checker: pass.
- Packaging shell syntax checks: pass.

## Manual Runtime Verification

Status: incomplete.

- The read-only `hardware-report` command ran successfully on the target machine.
- The report detected the target model/distro/kernel/desktop, NVIDIA GPU, four fan readings, missing asusd/supergfxd, read-only CPU/lighting/fan permissions, and the inactive session daemon.
- Daemon startup, UI startup, session DBus ownership/introspection, tray behavior, installed launcher/icon resolution, About actions, and browser/update flows were not verified in this audit.

## Hardware Validation Evidence and Scenarios

Status: incomplete and release-blocking for broad support claims.

- No completed machine record is committed under the hardware-validation docs.
- The generated report is a capture aid, not proof of write behavior.
- The target provides useful read-only evidence for missing asusd/supergfxd, NVIDIA telemetry, fan count `3+`, and permission-blocked controls, but a tester must still create and review a formal record.
- CPU writes, keyboard writes, profile control, battery limit, GPU switching, fan writes/restore, Aura/RGB, and hybrid-topology UI presentation were not exercised.
- Other checklist scenarios (fan counts 0/1/2, writable controls, other desktops/distros, tray behavior) remain unrepresented.

## Feature Verification

Status: automated model/contract tests pass; manual UI/hardware verification remains incomplete.

- DBus encoder/decoder tests cover state capabilities, setup rows, CPU rows, fan rows, lighting capability fields, optional telemetry, mixed integer widths, and unknown power-source values.
- Error/status categories are shared internally and tested without changing existing wire strings.
- No hardware setters were invoked during this audit.
- All checklist items that require observed UI behavior or a successful hardware write remain unchecked.

## Remaining Release Blockers

1. Complete an installed daemon/UI/session-DBus smoke test and verify tray/launcher/About/update behavior.
2. Add at least one reviewed hardware-validation record, then keep unsupported scenarios explicitly untested.
3. Run Flatpak, AppImage, RPM, and Arch checks in suitable clean build environments.
4. Install, upgrade, and uninstall the Debian and prefix artifacts in disposable test systems; verify activation/service paths and dependency resolution.
5. Perform the release-owner documentation and feature-advertising review across every checklist document.
6. Re-run all checks from a clean checkout at the intended release commit and record final sign-off.

No release, tag, commit, or push was performed.
