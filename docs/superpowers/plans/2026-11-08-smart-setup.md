# Smart Setup implementation plan

Goal: deliver the approved guided setup and safe manual native-package fallback.
Architecture: extend the typed registry and current session D-Bus status; preserve the hardware helper.
Tech stack: Rust, zbus, GTK4/libadwaita, existing TOML configuration.
Spec: ../specs/2026-11-07-smart-setup-design.md

## Constraints
No host install/remove/service changes, hardware writes, version changes or pushes.
No production fan records may be added. No package simulation is an execution guarantee.
Manual native-package review is required when locked transaction effects cannot be enforced.

## Tasks
- [ ] Core/provider registry and reviewed plan model, bounded read-only discovery, root/stale/concurrency/cancellation failure tests.
- [ ] Eight-stage GTK assistant, first-run persistence, normal-user guard, refresh/diagnostics/logs and honest manual-action guidance.
- [ ] Additive daemon report method and UI background transport with legacy fallback.
- [ ] Signature-validated asusd interface discovery preserving native fallback and hardware safety gates.
- [ ] Independent security review and documentation with before/after matrix and manual acceptance.
- [ ] Locked workspace fmt/build/tests/Clippy, packaging fixtures and metadata/workflow validation.

Review focus: malformed distro data; stale daemon contracts; plan ownership and root refusal;
native package backend limits; visible distinction between detection and physical support.
