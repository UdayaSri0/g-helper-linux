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

## Current implementation decision

- `has_fan_reading=true`
- `fan_count=4` (three ASUS-labelled rows plus one separate unlabeled ACPI row)
- `fan_mapping_confidence=unknown` overall; the three ASUS rows individually report `hardware_label`
- `fan_curve_readable=true`
- `fan_curve_writable=true` only when the verified direct or privileged route is available
- `has_fan_curves=true` only for safely mapped ASUS WMI channels with the complete eight-point ABI
- manual percentage, RPM target, sync control, and boost remain false; individual Auto/curve
  control is true only for the verified labelled channels

Generic `pwmN`, `pwmN_enable`, and `fanN_target` discovery remains diagnostic-only. Manual percent,
RPM-target control, fan sync, and boost are not exposed on this machine because no verified endpoint
implements those semantics.

The implemented ASUS WMI route additionally requires the `asus` RPM device and
`asus_custom_fan_curve` device to resolve to the same kernel device, exact CPU/GPU/Mid hardware
labels, eight complete readable point pairs, and enable state 1 or 2. IDs are semantic
(`asus-wmi:cpu`, `asus-wmi:gpu`, `asus-wmi:mid`) and callers never provide a path.

Curve writes validate all eight points, write and read back every temperature and PWM value, and
enable the curve only after the complete payload succeeds. Any partial failure attempts the driver's
factory-default/Auto command. Auto and reset use the verified driver command directly. Permission
failures fall back to `rog-helper-privileged` and PolicyKit action
`io.github.roghelper.fans.control`; telemetry does not contact PolicyKit.

The helper records active custom control in `/run/rog-helper/fan-control-active`. If the helper is
restarted after an interruption, reaches its idle timeout, or shuts down cleanly, it restores every
currently verified channel to Auto before clearing the marker. A hard failure that prevents both
the kernel and the restarted helper from running cannot be recovered in-process; firmware reboot
behavior remains the final safety boundary.

## Safety validation and remaining hardware work

- Shared validation enforces exact point count, temperature and percentage bounds, strictly
  increasing temperatures, non-decreasing duty, and conservative high-temperature floors.
- Provider writes are serialized. Discovery reads the complete layout before each write; each
  staged value is read back, enable happens last, and any failure invokes Auto reset.
- The daemon tries the unprivileged provider first and falls back only on `PermissionDenied`.
  Unsupported or unsafe mappings never invoke the helper.
- The UI distinguishes direct, authorization-required, authorization-denied, helper-missing,
  unsafe/read-only, telemetry-only, and unsupported states. Curve Apply is enabled only when an
  actionable route exists.
- Filesystem tests cover complete/incomplete layouts, trusted IDs, point-count rejection, direct
  success, readback conversion, Auto reset, fallback preference, authorization denial, and helper
  unavailability.
- Remaining hardware work is the supervised matrix below, especially real sysfs rollback behavior,
  helper interruption, suspend/resume, and firmware ownership interactions.

## Manual hardware validation still required

1. Confirm RPM-only telemetry with the helper stopped.
2. Install/start the helper and confirm diagnostics change to authorization-required.
3. Apply the conservative eight-point preview curve while thermally supervised.
4. Verify all eight point readbacks and the enabled state.
5. Return the selected fan to Auto and confirm firmware control resumes.
6. Deny and cancel separate PolicyKit prompts; confirm values remain unchanged and telemetry lives.
7. Stop and restart the daemon while a curve is active; confirm the helper remains the safety owner.
8. Stop the helper cleanly while a curve is active; confirm Auto restoration.
9. Kill and restart the helper with its marker armed; confirm startup restoration.
10. Exercise malformed curves and simulate an unavailable endpoint; confirm Auto is attempted and no
    incomplete curve is enabled.

## References

- [Linux hwmon sysfs interface](https://docs.kernel.org/hwmon/sysfs-interface.html)
- [Current Linux `asus-wmi.c` source](https://codebrowser.dev/linux/linux/drivers/platform/x86/asus-wmi.c.html)
- [Original ASUS custom fan-curve driver patch discussion](https://lkml.iu.edu/hypermail/linux/kernel/2109.0/03504.html)

**FAN WRITES IMPLEMENTED: NARROWLY.** Only verified ASUS WMI eight-point curves and Auto/reset are
implemented. Generic PWM, manual percent, RPM target, software curves, sync, and boost remain
unsupported. Hardware testing is required before release.
