# Smart Setup Assistant design and source audit

Status: approved by maintainer on 2026-11-08. Local implementation includes the guided diagnosis/manual-handoff path; automatic package transactions remain unavailable.
Audit baseline: remote main 06901e6dee073c7790f511a8b18de96121c4de85.
Target: G615JMR, Ubuntu 26.10, kernel 7.3.0-10-generic.
The target details are user-supplied context, not new physical validation.

## Intent and scope

Provide a normal-user first-run assistant that diagnoses the installation,
explains optional capabilities, previews reviewed package transactions and
verifies the result after explicit authorization. Preserve native battery and
Aura fallbacks, the existing daemon and hardware helper, and firmware recovery.

Implementation must update the shared model, D-Bus keys/encoding/decoding,
provider probes, daemon dispatch, UI, CLI diagnostics, fixtures and documentation
together. Existing clients must tolerate additive fields and new unknown states.

No version or release changes, pushes, host service installation or physical
hardware writes are authorized by this task.

## Current implementation

| Area | Source evidence | Current behavior and missing work |
| --- | --- | --- |
| Typed status | crates/rog-core/src/model.rs, DependencyKind, DependencyState, SetupStatus | Five service kinds and permission states exist. No installation plan or transaction model. |
| Setup probes | crates/rog-providers/src/setup.rs | Bounded service and API probes, inactive/missing distinctions, optional-provider reconciliation. Distribution, trusted package candidates and service conflicts need unified representation. |
| Existing UI | crates/rog-ui/src/main.rs, Setup & Access section | Installation health, helper/PolicyKit rows, optional services, hardware, refresh, diagnostics, stale session-daemon restart. No first-run sequence, plan review or package transaction progress. |
| Daemon contract | crates/rog-daemon/src/main.rs, GetSetupStatus | Reuse the normal session service for typed orchestration and preserve compatibility. |
| Privileged API | crates/rog-core/src/privileged.rs; crates/rog-privileged/src/main.rs | API version 3, typed CPU/battery/fan/Aura methods, PolicyKit checks and structured errors. No package-install interface. |
| Caller identity | crates/rog-privileged/src/main.rs, check_polkit_authorization and category authorization methods | PolicyKit system-bus-name subject comes from the actual D-Bus sender. Never accept a UI-provided caller identity. |
| Hardware helper sandbox | packaging/systemd-system/rog-helper-privileged.service | Root, AF_UNIX only, strict filesystem protection, narrow hardware paths. This service is unsuitable for package management. |
| Owned integration | packaging/scripts/package-common.sh, stage_privileged_integration | Native packages already include binaries, session/system D-Bus activation, units, policies, icons and metadata. Reuse the inventory. |
| Package lifecycle | packaging/debian/postinst; packaging/rpm/rog-helper.spec.in | Reload/restart integration through package lifecycle. Privileged helper is D-Bus activated, not an enable-at-boot service. |
| Aura udev | packaging/udev/60-rog-helper-aura.rules | Exact-device stable alias, no global permissions or user hidraw access. Preserve this boundary. |
| asusd | crates/rog-providers/src/asusd.rs | Static endpoint candidates plus live property/capability probes. Needs bounded introspection and signature-aware contracts. |
| Fans | crates/rog-providers/src/hwmon.rs and fan_write_policy.rs; privileged helper recovery | Mapping, ownership and durable recovery exist. Production fan verification table is empty, so custom writes remain unavailable until reviewed physical evidence exists. |

Current main includes newer setup-status improvements after v0.4.1. The assistant
must build on those improvements rather than replace them.

## Approaches

1. Recommended: typed orchestration in the session daemon, native PackageKit
   transactions for an explicit package allow-list, and the existing helper only
   for typed hardware operations. Authorization stays with the component that
   owns the operation.
2. Separate custom privileged installer service. This can support stricter
   distro-specific transactions but adds a root-facing API, source validation,
   package locking and recovery code. Defer unless native APIs cannot satisfy a
   concrete supported operation.
3. A diagnosis-only wizard with native package-manager handoff. Safe fallback
   wherever trusted package resolution, repair or transaction preview is
   unavailable. It cannot claim a component was installed until re-probed.

Do not use the development sudo/copy installer as an in-app repair backend.

## Registry and plans

Extend the existing typed model with a system snapshot and component records.
Record distro ID/version, kernel, DMI identity, drivers, installed version,
runtime/API status, ownership/conflicts and hardware verification separately.
Distinguish missing optional, inactive, incompatible, unsupported distribution,
unsupported hardware, authorization required and unverified write access.
Normal per-operation authorization is informational.

Define a typed setup plan with an opaque ID, snapshot revision, allow-listed
component operations, package IDs/versions, repository identity, complete
simulated transaction, services affected, authorization, conflicts, reboot
requirements and expected capabilities. No shell strings, arbitrary package
names, file paths, repository URLs or service names may be executed from UI
input.

Use distro-native package metadata and only-trusted PackageKit transactions.
A signed package alone does not establish an approved source. Validate the
repository against a reviewed distro policy. Resolve exact candidates, simulate
all changes and reject removals or unreviewed dependencies. Revalidate before
execution and require renewed review when candidates or effects change.
Simulation is not an atomic promise of the later dependency graph. An executable
backend must enforce the reviewed effect set after acquiring its transaction
lock. If the backend cannot provide that guarantee, use the native package
manager's interactive review as a manual handoff. Never label a preflight
simulation as an exact execution guarantee.
Unknown distributions and unavailable backends get a manual explanation.

Read-only host apt-cache inspection found no candidate for asusctl/asusd or
supergfxctl/supergfxd on the target Ubuntu 26.10 configured sources. Consequently
automatic ASUS package installation is unavailable there. Do not add a PPA,
compile external source, install legacy GPU managers or switch GPU state.

Repair checks the owned payload against the package inventory, then offers a
native package reinstall only if the backend can preview and safely execute
that operation from a trusted source. Otherwise show the specific package repair
instructions. PackageKit cannot be assumed to support reinstall on every backend.
A locally downloaded ROG Helper DEB must not silently become a trusted repository
candidate. Mark unsupported repair actions as manual instead of reporting success.

Refuse GUI startup and setup execution when the effective UID is root, before
opening GTK or making a setup request. The normal-user daemon must derive each
request's session-bus principal from its D-Bus sender. Bind plans, execute,
cancel and log access to that principal and its user session; an opaque plan ID
does not authorize an operation. PackageKit sees the daemon as its caller, so
validate the original client before forwarding a reviewed native operation.

Keep transaction state in the daemon with an installation lock, bounded logs,
stable operation ID and clear terminal states. A second requester gets Busy.
Cancellation follows the native transaction's AllowCancel state. Once commit
cannot be interrupted safely, explain that and wait for completion. Reconnect
after client interruption using the same daemon-owned operation ID. Persist
minimal operation metadata atomically under the existing per-user state
directory, mode 0600 in a mode-0700 directory. Store plan revision, state,
package IDs and the backend transaction identity, without credentials or
unredacted personal paths. Backend restart or daemon interruption invalidates
the old execution authorization. Reconnect only when transaction identity and
ownership can be established; otherwise mark the result unknown, consult the
native package-manager state and perform fresh read-only probes. Never replay
an interrupted installation automatically.

## Assistant experience

Use the existing libadwaita groups and theme in a focused assistant page.
Show it once per user unless completed/skipped, and allow reopening from Setup.
A saved first-run choice must not assert current installation health.

Eight stages:
1. Welcome and read-only system detection, with Check System.
2. Core installation health and the normal user session service.
3. Optional ASUS integrations with missing/unsupported guidance.
4. Hardware readiness with separate detection, authorization and validation.
5. View Setup Plan, including exact packages, sources and complete effects.
6. Explicit Install Supported Components or Repair ROG Helper Integration,
   native authorization, progress and safe cancellation.
7. Recheck Services and verify actual API readiness.
8. Troubleshooting and finish, with View Installation Log and Copy Diagnostics.

Actions remain disabled with an explicit reason when no safe transaction exists.
Use wrapping labels, keyboard focus order, accessible names, a cancellable
background probe and expanders for technical paths and logs. Collapse repeated
status rows into a single component card. Preserve existing diagnostics redaction.
There is no authorization prompt or installation on page open.

## ASUS and fan integration

Use existing bounded D-Bus introspection utilities to validate known interfaces,
property/method signatures and available features. An interface version string
alone is insufficient. Track API incompatibility separately from stopped service.
Use introspection fixtures for supported versions and reject unknown writes.

Detect active competing platform/power/GPU owners and show a conflict plan.
Changing owners is a separately reviewed action, never a probe side effect.
Keep supergfxd optional and legacy. Cardwire gets a read-only experimental
provider classification, without installation or GPU transitions.

Extend the existing ASUS WMI discovery and verification policy. Validate
canonical device and driver relationships, stable CPU/GPU/Mid mapping, eight
points, units/ranges, identity, ownership, readback and firmware Auto semantics.
Do not infer third-channel support from filenames alone. Simulated records stay
test-only. A package install cannot populate the production verification table.

Only reviewed supported mappings can expose Apply. Existing helper methods must
continue to authorize, validate, lock, back up, write, verify readback and recover
to firmware Auto. Keep marker-owned restart/recovery scope. Physical qualification
requires a separate approved manual procedure recording BIOS/kernel/device,
temperature and duty units, readback, Auto restoration, interrupted operation
and suspend/resume behavior before a production record can be proposed.

## Validation and delivery

Tests must cover distro/source allow-lists, malformed inputs, simulated
dependencies/removals, stale plan revisions, source changes, missing/stopped/
incompatible services, ownership conflicts, first-run migration, root-GUI
refusal, concurrent requests, cancellation boundaries, package failure,
PolicyKit denial and post-install verification. Fan tests use mock sysfs and
test-only verification records. Existing deny-by-default cases must stay denied.

Run locked workspace formatting/build/tests/Clippy, focused provider and UI
tests, isolated D-Bus tests, package payload checks, DEB/RPM lifecycle tests,
AppStream/desktop validation and workflow tests. Do not run system installation,
GUI/daemon as root or physical control writes on the development host.

Update architecture, D-Bus API, permissions, build/install, GUI, provider,
hardware support and troubleshooting documents alongside the code. Include a
before/after matrix distinguishing available, software-tested, physical-unverified,
experimental and unsupported. Installation success never upgrades hardware
validation status.

## Release decision

Current readiness for this proposed feature is NO-GO. The assistant and
transaction engine do not exist yet, trusted automatic ASUS installation is
unavailable on the target distro, and custom fan writes lack physical
qualification. Those constraints must remain visible in the delivered feature.

## Primary external references

- https://github.com/OpenGamingCollective/asusctl/blob/main/README.md
- https://github.com/OpenGamingCollective/asusctl/blob/main/docs/getting-started/supported-distributions.md
- https://packagekit.freedesktop.org/doc/enum.TransactionFlagEnum.html
- https://www.freedesktop.org/software/PackageKit/doc/concepts.html

The maintainer approved approach 1 with approach 3 as an explicit per-operation fallback. Current delivery uses approach 3 because no backend enforces the reviewed locked transaction effects.
