# Fan-control backend discovery

Date: 2026-08-13

This report records a read-only investigation on the target ASUS laptop. No sysfs value was written,
no DBus setter was called, no module was changed, and no permission was modified.

## Target evidence

- Product: `ROG Strix G16 G615JMR_G615JMR`; board `G615JMR`; BIOS `G615JMR.318`.
- Kernel: `7.0.0-28-generic`.
- No `asusd` or `supergfxd` system-bus name or installed system service was found, so there is no
  installed ASUS service interface to introspect for fan control.
- The in-tree `asus_wmi`/`asus_nb_wmi` stack exposes an `asus` hwmon device with three RPM inputs:
  `fan1=cpu_fan`, `fan2=gpu_fan`, and `fan3=mid_fan`. The labels come from the kernel, so RPM mapping
  confidence for those three rows is `hardware_label`.
- A separate `acpi_fan` hwmon device exposes one unlabeled RPM input. It is retained as an independent
  `Fan 1` row; it is not guessed to be a duplicate or assigned to CPU/GPU/Mid. Because this fourth row
  is unlabeled, aggregate mapping confidence remains `unknown`.
- That telemetry device exposes `pwm1_enable`, `pwm2_enable`, and `pwm3_enable`, but no matching
  `pwm1`, `pwm2`, or `pwm3` duty files. The enable files are root-owned and are not treated as a
  complete manual-percent interface.
- A sibling hwmon device named `asus_custom_fan_curve`, backed by the in-tree `asus_wmi` driver,
  exposes three channels. Each channel has eight readable paired
  `pwmN_auto_pointM_temp`/`pwmN_auto_pointM_pwm` attributes plus `pwmN_enable`.
- The curve attributes are mode `0644`, owner `root:root`; the current session daemon user cannot
  write them. No write attempt was made.

Observed read-only curves, preserved as raw ABI values:

| Channel | Temperatures | PWM values |
| --- | --- | --- |
| 1 | 40, 55, 64, 68, 71, 74, 77, 80 | 22, 28, 40, 61, 73, 99, 112, 145 |
| 2 | 40, 55, 64, 68, 71, 74, 77, 80 | 15, 48, 56, 68, 76, 94, 114, 147 |
| 3 | 40, 55, 64, 68, 71, 74, 77, 80 | 7, 7, 61, 86, 91, 102, 112, 122 |

The standard Linux hwmon ABI defines `pwmN` as a 0–255 duty value, `pwmN_enable=1` as manual mode,
and `pwmN_enable=2+` as chip-specific automatic control. It also defines paired automatic curve
attributes, but their exact behavior remains driver-specific. The `asus_wmi` source has separate
CPU, GPU, and Mid fan-curve availability and restore paths. This is credible evidence for a readable
ASUS kernel interface, not sufficient evidence for a safe application write contract.

## Current decision

- `has_fan_reading=true`
- `fan_count=4` (three ASUS-labelled rows plus one separate unlabeled ACPI row)
- `fan_mapping_confidence=unknown` overall; the three ASUS rows individually report `hardware_label`
- `fan_curve_readable=true`
- `fan_curve_writable=false`
- `has_fan_curves=false` because the existing UI interprets this as an actionable curve backend
- manual percentage, RPM target, individual control, sync control, and boost remain false

Generic `pwmN`, `pwmN_enable`, and `fanN_target` discovery is diagnostic-only. A provider no longer
opens a candidate file for writing merely to probe it, and setter calls are rejected before sysfs.

## Implementation plan after the write gate is satisfied

### `rog-core`

- Keep reading, curve reading, and curve writing as separate capabilities.
- Model the backend's exact fixed point count and raw ranges; do not silently interpolate or discard
  points.
- Retain strict increasing-temperature validation, non-decreasing duty validation, conservative
  high-temperature floors, and add backend-reported bounds before serialization.
- Represent CPU/GPU/Mid mapping confidence explicitly; never promote index correlation alone to an
  authoritative mapping.

### `rog-providers`

- Add a dedicated ASUS WMI curve provider, selected only by a positive driver/device probe and a
  supported ABI layout. Do not put writes back into the generic hwmon telemetry provider.
- Read all points and enable state before a transaction. Reject partial layouts, missing pairs,
  unexpected point counts, values outside the driver range, and unsupported channels.
- When writing is eventually authorized, stage and verify every point, then enable the curve only
  after the complete payload succeeds. On any error, invoke the verified firmware/BIOS Auto restore
  operation and confirm its state.
- Do not claim writable based on mode bits. Require a deliberate privilege design (for example a
  narrowly scoped system service/polkit method) and verify actual access without probe writes.

### `rog-daemon` and DBus

- Keep the current string-keyed maps and add keys without renaming old ones.
- Expose fixed point count, channel identity evidence, supported raw ranges, current curves, current
  enable state, and restore support.
- Serialize fan writes through one transaction lock. Re-read after apply. Return `NotSupported` for
  unverified hardware, `InvalidArgs` for malformed/safety-violating curves, and `AccessDenied` for a
  verified backend lacking privilege.
- Cache the pre-change state and restore firmware Auto on partial failure, critical-temperature
  safety events, and orderly daemon shutdown. Startup must never apply a saved curve implicitly.

### Cooling UI

- Show the currently readable firmware curves as read-only only after channel mapping is verified;
  until then show candidate channel numbers and mapping confidence.
- Enable editing/apply/reset controls only when `fan_curve_writable=true`, restore support is true,
  and backend bounds are present.
- Preview validation errors before submission, require an explicit Apply action, and show the
  verified post-write state. RPM telemetry alone must never enable curve controls.

### Tests and fallback behavior

- Use filesystem fixtures; tests must not require ASUS hardware or root.
- Cover complete and incomplete point sets, unreadable/non-numeric values, unexpected channels,
  label confidence, monotonicity, bounds, partial transaction failure, read-back mismatch, and Auto
  restore.
- Add a fake backend transaction test proving that failure at every write step restores Auto and
  never leaves an incomplete curve enabled.
- On unsupported ABI, permission loss, mapping uncertainty, or restore failure, keep telemetry
  visible, disable all controls, emit actionable diagnostics, and prefer firmware/BIOS Auto.

## Data still required before fan writes

1. Confirm the exact `asus_wmi` ABI semantics and raw units for this kernel version from the matching
   kernel source/config, including the meaning of enable values and factory-default/Auto restoration.
2. Establish and test a least-privilege daemon write path; the current user cannot write the files.
3. Verify the relationship between the three labelled RPM sensors and the three curve channels by
   authoritative driver/device evidence, not index coincidence alone.
4. Capture read-only state before/after normal firmware profile changes to understand ownership and
   whether another service or firmware rewrites curves.
5. Test apply, read-back, partial-failure recovery, daemon shutdown, suspend/resume, and reboot on
   disposable hardware under thermal supervision before exposing UI controls.

## References

- [Linux hwmon sysfs interface](https://docs.kernel.org/hwmon/sysfs-interface.html)
- [Current Linux `asus-wmi.c` source](https://codebrowser.dev/linux/linux/drivers/platform/x86/asus-wmi.c.html)
- [Original ASUS custom fan-curve driver patch discussion](https://lkml.iu.edu/hypermail/linux/kernel/2109.0/03504.html)

**FAN WRITES IMPLEMENTED: NO.** The target exposes a credible readable kernel curve ABI, but lacks an
installed service contract, current-user write access, authoritative curve-channel mapping, and a
fully tested failure/Auto-restore transaction.
