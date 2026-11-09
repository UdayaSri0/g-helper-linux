# Smart Setup engineering report

Baseline: 06901e6dee073c7790f511a8b18de96121c4de85.
Delivery is local, uncommitted source and documentation.

## Delivered behavior

The eight-stage assistant starts on first launch, persists Finish/Skip, can be
reopened and displays actual read-only refresh progress, manual plan guidance,
diagnostics and verification. Existing core service/permission/hardware rows
remain available. UI buttons explain the unavailable automatic install path.

A typed registry/snapshot includes bounded distro/kernel/DMI/driver observations,
PackageKit integration presence and advisory power/GPU process observations.
Stable revisioned manual plans are available through the additive TOML-encoded
GetSetupAssistantSnapshot method and CLI setup-check reports. The custom config
loader preserves the new preference with legacy default false.

GUI and session daemon reject effective UID zero. The hardware helper and its
packaging/PolicyKit/udev boundaries remain as before. asusd property writes now
validate bounded XML introspection and exact property signatures. Setup reports
an incompatible service distinctly from a stopped/unreachable service.
roxmltree 0.20.0 is the one new dependency, for real XML validation at that boundary.

## Changed source groups

- crates/rog-core/src/setup.rs and lib.rs: typed system/plan/snapshot.
- crates/rog-core/src/config.rs: first-run preference and loader/roundtrip test.
- crates/rog-core/src/model.rs: incompatible dependency state and mapping test.
- crates/rog-providers/src/setup_assistant.rs and lib.rs: bounded observation,
  manual planning, reporting and fixtures.
- crates/rog-providers/src/asusd.rs and setup.rs: validated contracts and
  incompatible-service status with XML and capability tests.
- crates/rog-daemon/src/main.rs: read-only snapshot API and root refusal.
- crates/rog-ui/src/setup_assistant.rs and main.rs: guided UI, refresh and
  typed snapshot decoding, root refusal and tests.
- crates/rog-cli/src/main.rs: setup-check prints the shared system/plan report.
- Cargo.lock, provider/UI manifests: parser dependency and existing TOML reuse.
- docs/SMART_SETUP.md plus architecture, API, permissions, build, GUI, provider,
  support and troubleshooting references; approved design and execution plan.
- Graphify metadata updated according to repository instructions.

## Validation

Passed locked workspace build, formatting, 393 Rust tests, and all-target
Clippy with warnings denied. Passed git diff whitespace validation.
Passed install-dev and Debian maintainer-script fixtures, AppImage GTK and tool
integrity fixtures, seven package integration tests, twelve release environment/
asset tests, release workflow guards and metadata validation.

The isolated session smoke test passed with helper/asusd/supergfxd unavailable.
Its GUI process remained alive for five seconds; this does not assert visual
layout or keyboard interaction.

RustSec policy and its vulnerable-fixture rejection passed using advisory
database revision 26fd2d43f7280a1fb925a3743ec463017afe7119. Four unmaintained
notices remain (pin-utils, zbus_names, zvariant and zvariant_derive). This was a
cached database audit, not a claim of latest advisory database synchronization.

## Scope and release assessment

The full automatic installer request is not complete. There is no executable
native package transaction backend, reviewed package/version/source resolution,
transaction cancellation/denial/concurrency handling, durable installation-job
recovery or interactive package-manager launch. All plans are non-executable
manual guidance under the approved fail-closed fallback. Tests do not claim to
exercise those absent installer operations.

Accordingly this is NO-GO for advertising an automatic installer. It is a local
candidate for guided diagnosis and manual setup review, subject to desktop UI
acceptance and package validation. Hardware writes and physical qualification
remain separate. No production fan records were added.

## Manual acceptance

Use a normal desktop user with isolated XDG configuration. Confirm first-run
Setup routing, each stage, Back/Next, wrapped labels/buttons, keyboard traversal,
Skip persistence, Finish persistence and compact Reopen. Refresh with a compatible,
older or missing daemon and inspect clear fallback messages and clipboard data.

Review core vs optional status on the G615JMR without asusd/supergfxd. Confirm
native battery/Aura observations remain and unverified fan Apply stays disabled.
Observe that opening or advancing setup never authenticates or changes packages,
services, drivers, GPU ownership or hardware. Any real installation/reinstall
or physical write requires separate maintainer authorization.

## Final development package results

DEB and tarball built successfully into /tmp/rog-setup-package-check.gpUeWw.
DEB metadata, dependencies, exact contents, ownership/modes and security validator
passed. Synthetic lifecycle plus real dpkg install/configure/upgrade/remove/purge
passed in unshare user namespaces. All five payload manifest hashes and tarball
member/link checks passed. These are development artifacts, not release assets.

RPM and full AppImage builds were skipped because required host tools are absent.
Their script fixtures passed. No host tool installation was attempted.
