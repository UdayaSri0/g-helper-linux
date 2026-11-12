# Guided setup and hardware integration

Normal launches open Dashboard. The first-run assistant is available from
Setup & Access, including when its review is incomplete. It guides the desktop
user through system discovery, core integration, optional providers, hardware
readiness, a review plan, progress, verification and finish. Completing the review
is a per-user preference. Reopen Setup & Access to inspect current health.

## Installation workflow

1. Install the native ROG Helper package with the distribution package manager.
   The package owns its application binaries, user session daemon, system helper,
   D-Bus activation/policy, PolicyKit actions, exact Aura alias, icons and metadata.
2. Launch the GUI as the desktop user and run Check System.
3. Review the plan and individual provider guidance. Optional missing providers
   do not make the core installation unhealthy.
4. Review any package installation or reinstall in the native package manager.
   A package's presence alone does not prove a compatible service API.
5. Recheck Services and inspect remaining diagnostics before using a control.

The assistant's current backend provides a **manual native-package handoff**.
It does not execute package installs or repairs. PackageKit simulation does not
guarantee that the later locked transaction applies exactly the reviewed graph.
Install actions stay unavailable until a backend can enforce that contract.
There are no automatic repository changes or root commands from the UI.

Ubuntu 26.10 remains usable with the existing native fallbacks. No verified
asusctl/asusd package source is configured by ROG Helper. The assistant explains
manual distribution support review. Legacy supergfxd is optional. Cardwire is
experimental and receives no automatic installation or GPU transitions.

For broken owned integration, review a reinstall of the original verified native
package. A downloaded DEB is not automatically a trusted repository candidate.
Use the existing user-session restart action for a stale session daemon. The
privileged helper starts through system D-Bus activation rather than being
enabled as a boot service.

## Architecture and security

Existing typed SetupStatus remains the service-readiness contract. The additive
GetSetupAssistantSnapshot session D-Bus method supplies a TOML-encoded typed SetupSnapshot system/plan. Discovery reads bounded operating-system metadata and observes known
power/GPU processes. An installed PackageKit service is reported as installed,
not proof that its API or transaction solver is usable.

The normal-user GUI displays this report and performs checks in its background
worker. The GUI and session daemon refuse root startup. Hardware operations use
the existing typed privileged helper with category-specific PolicyKit checks.
Opening setup does not request authentication. Normal authorization prompts are
not installation failures.

asusd writes now validate bounded runtime introspection XML and the exact
property name, type and write access expected by the supported endpoint.
Unexpected contracts fail closed. A new XML parser dependency is used at this
trust boundary. Existing endpoint aliases and native battery fallback remain.

## Before and after

| Area | Existing behavior | This implementation |
| --- | --- | --- |
| Setup status | Typed service/permission diagnostics | Reused, with guided eight-stage review |
| First-run UI | Dashboard and Setup page | Dashboard opens by default; setup review remains available with persisted completion |
| System inventory | Individual provider probes | Distro/kernel/DMI/ASUS driver and observed manager inventory |
| Installation plan | Text recommendations | Typed manual-review actions with reasons and stable revision |
| Package mutation | Native package manager outside app | Explicit safe handoff; automatic execution unavailable |
| asusd contract | Endpoint and value probes | Runtime XML/signature checks before writes |
| Fan mapping/recovery | Conservative WMI mapping and transactional recovery | Reused; production custom-write records still required |

## Validation and manual acceptance

Automated tests cover the new system fixtures, unsupported distro plans,
malformed metadata, stable plan revision, GUI navigation and root parsing,
first-run configuration compatibility and asusd signature failures. Existing
mock hardware tests remain authoritative for software recovery paths.
Manual package transactions are outside this backend, so package-manager
failure/cancellation/authorization and concurrent transaction tests must not be
reported as exercising a new installer.

Run the locked workspace fmt/build/tests/Clippy and packaging fixtures.
For UI acceptance use an ordinary desktop session or isolated Xvfb session.
Verify first-run review, keyboard navigation, wrapping layout, clipboard output,
refresh progress, daemon absence and older-daemon fallback. Confirm setup itself
does not authenticate or mutate package/service/hardware state.

Physical custom fan-curve validation is a separate approved procedure. The
production verification table is empty. Inspect canonical driver/device/channel
identity and read-only curves first. Any later write test needs explicit
approval, thermal observation, backup, readback, firmware Auto restoration and
interruption/restart/suspend evidence before a production support record.
ASUS WMI detection alone never enables custom writes.

## Remaining limitations

Automatic reviewed native package transactions, durable installation-job
recovery and interactive native installer handoff are not implemented. The
assistant gives manual guidance. Its reports cannot establish physical
hardware compatibility, and observed processes are advisory ownership evidence.
No release-readiness claim for an automatic installer follows from this change.
