# Issue 85 fan-write support policy

This is a software authorization policy, not evidence of physical hardware validation.

## Production status

The production custom-curve support-record table is empty. Consequently no current machine may
enter `verified_write` and custom Apply must fail before the first curve endpoint write. Test-only
records exercise the positive path without approving their synthetic identities for production.

The decision states are:

| State | Meaning | Custom Apply |
| --- | --- | --- |
| `read_only` | Telemetry or curve readback is available without a write route | Denied |
| `unverified` | A plausible/writable ABI exists but no exact approved physical record matches | Denied |
| `unsupported` | Identity, backend, ABI, or channel evidence contradicts policy | Denied |
| `verified_write` | An approved record and every current pre-write check match | Allowed only through the bounded transaction |

Each decision includes a machine-readable reason and a diagnostic explanation. Detection,
readability, endpoint permissions, helper readiness, physical-validation status, current Apply
authorization, and recovery availability are reported independently.

## Record admission

A production record requires committed supervised physical evidence for the exact system vendor,
DMI model/product, board vendor/name, BIOS vendor/version, kernel release/build, driver identity,
ASUS WMI device, channel roles/labels, eight-point ABI, safe envelope/minimum duty, readback, Auto
restore, interruption/restart recovery, and suspend/resume behavior. The record must name its test
date, tester, evidence references, and maintainer review status.

Issue #64 supplies supervised fan behavior and recovery evidence. Issue #65 supplies the dated,
redacted machine record. A reviewed change adds the integrity-protected production record only after
both are complete. BIOS, kernel, driver, board, backend, or channel changes fail closed. The current
implementation accepts exact values only; any future compatibility-range design requires its own
evidence, review, and deterministic tests. Product-family prefixes, write acceptance, and
minimum-kernel guesses are not sufficient.

## Enforcement and feature separation

The provider and privileged helper independently evaluate the live environment for every Apply.
They do not trust a caller-supplied model string or cached startup capability. Curve validation,
endpoint ownership/type checks, transaction locking, snapshot/readback, recovery markers, PolicyKit,
and rollback remain additional requirements after policy authorization.

RPM and temperature telemetry, raw PWM, current-curve readback, Import Current, draft editing, and
local preset storage remain available without a production record. Import Current performs no write
and does not activate its draft. Auto/BIOS recovery is separately restricted to the recognized reset
endpoint and exact semantic channels; it does not confer custom-write authorization and denied Apply
must not reset unrelated channels.

## Residual limits

Hardware-free tests can prove fail-closed routing and zero simulated writes, but cannot establish fan
response, thermal safety, firmware interpolation, real readback semantics, suspend behavior, or
successful Auto restoration. Those claims remain blocked on supervised evidence and independent
review.
