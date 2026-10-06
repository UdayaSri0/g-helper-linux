# Fan-control backend discovery

Date: 2026-10-02

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
- manual percentage, RPM target, operational sync control, and boost remain false; individual Auto/curve
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

Current curve reading is now exposed through `GetFanState` and `GetFanCurves`. It uses the same exact
identity and eight-pair gate, preserves each raw `0..=255` PWM value, reports the converted percentage
and enable mode, and performs no write or authorization request. Its `backend_current` provenance
means only that the values came from current backend readback; it does not claim factory authorship
or ownership. The Cooling page can copy this data into a local draft with **Import Current**. That
operation is distinct from **Restore Auto**, which uses the driver's command `3`; an imported curve
is not labelled as a factory default.

Quiet, Balanced, and Performance are conservative application-provided draft templates, not ASUS
factory modes. Selecting one only updates the local eight-point preview. Apply still uses the shared
validator and no preset is automatically written. Reset Draft returns the local preview to Balanced
without touching hardware.

The helper records active custom control in `/run/rog-helper/fan-control-active`. Marker replacement
is atomic and durably synced, and the marker records the exact semantic channel IDs. While it remains
armed, the helper stays resident instead of taking its normal idle exit. On startup after a crash or
kill, or during clean SIGINT/SIGTERM shutdown, it attempts every recorded channel and clears the
marker only after the complete recorded set returns to Auto. A missing or failed channel leaves the
exact marker set armed for retry. `RuntimeDirectoryPreserve=yes` retains that state across service
restart and stop/start cycles.

Automatic daemon safety restoration tries direct Auto first, then the helper's marker-gated recovery
method. Recovery performs the `io.github.roghelper.fans.control` PolicyKit check without allowing
user interaction: it never opens a prompt and succeeds only when the caller is already authorized.
It cannot enable a curve or act without a root-owned marker. A hard failure that prevents both the
kernel and the restarted helper from running cannot be recovered in-process; firmware reboot behavior
remains the final safety boundary.

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
- Filesystem tests cover complete/incomplete and extra-point layouts, label and canonical-device
  mismatch, trusted IDs, raw-preserving read-only import, point-count rejection, direct success,
  readback conversion, Auto reset, fallback preference, authorization denial, helper unavailability,
  enable-last ordering, injected mid-transaction failure, readback mismatch, and rollback failure.
  Complete recovery-marker startup/idle/shutdown lifecycle coverage remains follow-up test work.
- Remaining hardware work is the supervised matrix below, especially real sysfs rollback behavior,
  helper interruption, suspend/resume, and firmware ownership interactions.

## Hysteresis research outcome

Fan hysteresis is **unsupported**. Mainline Linux at commit `ce1e0223d8ad` exposes no ASUS
hysteresis attribute or device ID, and asusctl/asusd at commit `28b456dec296` has no hysteresis API.
The generic hwmon documentation's optional `*_temp_hyst` vocabulary is not implemented by
`asus-wmi`. Windows G-Helper's proprietary firmware behavior was reviewed only as a UX reference;
its GPL implementation and firmware method were not copied or routed through the helper. Software
hysteresis was also rejected because it would fight firmware with a new active thermal controller.

## Manual hardware validation still required

1. Stop the privileged helper and confirm RPM telemetry plus current-curve import remain readable.
2. Start the helper and confirm helper readiness is reported without an authorization prompt.
3. Confirm permission-blocked verified channels report `authorization_required`.
4. Apply one conservative eight-point draft while thermally supervised; do not stress the machine.
5. Record all eight temperature/PWM readbacks, raw and percentage values, and enabled state.
6. Use Restore Auto and confirm firmware/profile-controlled behavior resumes.
7. Cancel and deny separate PolicyKit prompts; confirm values remain unchanged and telemetry lives.
8. Restart the user daemon with a curve active; confirm the helper remains the safety owner.
9. Stop the helper cleanly with a curve active; confirm all-channel Auto and marker removal.
10. Kill and restart the helper with its marker armed; confirm startup recovery and marker removal.
11. Suspend and resume with a supervised curve; confirm ownership and successful Auto restoration.
12. Simulate backend disappearance during a staged transaction; confirm no partial curve is enabled,
    rollback failure is surfaced, and the marker remains armed when full recovery is unproven.
13. Change the firmware/platform profile while a curve is active and confirm the ownership transition
    is safe and accurately reported.

## References

- [Linux hwmon sysfs interface](https://docs.kernel.org/hwmon/sysfs-interface.html)
- [Linux `asus-wmi.c` at reviewed commit `551c722f4080`](https://github.com/torvalds/linux/blob/551c722f40809618230001baccf219193e22fc5a/drivers/platform/x86/asus-wmi.c)
- [Linux `asus-wmi.c` hysteresis audit at `ce1e0223d8ad`](https://github.com/torvalds/linux/blob/ce1e0223d8ad4211275c82a17ed6d43ab81e13d9/drivers/platform/x86/asus-wmi.c)
- [asusctl/asusd fan curves at `28b456dec296`](https://github.com/OpenGamingCollective/asusctl/tree/28b456dec2969e293e0d8eae7a8face629ec4be2/rog-profiles)
- [Windows G-Helper behavioral reference at `54c5bd00da82`](https://github.com/seerge/g-helper/tree/54c5bd00da82e20a0361228e5758f692b3b9560b/app)
- [Original ASUS custom fan-curve driver patch discussion](https://lkml.iu.edu/hypermail/linux/kernel/2109.0/03504.html)

**FAN WRITES IMPLEMENTED: NARROWLY.** Only verified ASUS WMI eight-point curves and Auto/reset are
implemented. Generic PWM, manual percent, RPM target, software curves, sync, and boost remain
unsupported. Hardware testing is required before claiming hardware-validated fan support or making
fan curves a dependency of persistent profiles or automation.
