# Issue 87 safety work log

This ledger tracks software changes and evidence for epic #87. It is not a
hardware-safety certification. The initial audit was performed against
`main` commit `51de2df` on 2026-10-25; the historical comparison point is
`6414c3e`.

## Current status

| Issue | Status | Evidence / next gate |
| --- | --- | --- |
| #79 | In progress | Write policy now requires explicit terminal coverage; supervised hardware evidence remains blocked. |
| #85 | Blocked | Generic hwmon discovery is not physical support evidence; exact model/kernel/BIOS policy still needed. |
| #86 | In progress | Re-apply while custom mode is active is rejected before writes; atomic firmware transition remains unverified. |
| #80 | Open / design pending | No independent daemon lease recovery has been implemented yet. |
| #82 | Open / design pending | Existing recovery marker is retained; direct-write ownership persistence needs a separate change. |
| #81 | Open / design pending | Role-sensor and stall failsafes need a separate fake-backend workstream. |
| #64 | Blocked | Requires supervised physical Apply/Auto/rollback/restart evidence; no writes were performed by this audit. |
| #65 | Blocked | No new machine validation record or independent sign-off is available. |
| #83 | Open | CPU transaction work not yet started. |
| #84 | Open | GPU readiness/mode-mapping work not yet started. |
| #63 | Blocked | Requires supervised G615JMR Aura observations; no physical writes were performed. |
| #66 | Blocked | Installed-runtime matrix requires appropriate distro environments. |
| #71 | Open | AppImage provenance work is planned as an independent release-security PR. |
| #87 | Open | Epic cannot close while safety, hardware evidence, and maintainer sign-off gates remain incomplete. |

## Software changes in this workstream

- `FanCurvePolicy::verified_asus_write()` is an explicit fail-closed policy
  requiring eight points and a terminal point at or above 95°C. This does not
  claim that every ASUS firmware curve must use that endpoint.
- Read-only/default validation remains available for firmware curve discovery;
  imported factory curves are not silently converted into writes.
- Re-apply is rejected when the readback reports custom mode already active,
  before any point write occurs, until an atomic transition is verified.
- Pure unit and fake-hwmon tests cover the low-duty/low-endpoint regression and
  active-mode no-write behavior.

## Evidence boundary

Automated tests use domain values and temporary fake hwmon files only. They do
not establish thermal behavior, fan movement, firmware interpolation, restore
success on a real machine, or model compatibility. Physical gates remain open
and must be completed by a supervised human tester with independent review.
