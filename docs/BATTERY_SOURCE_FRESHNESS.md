# Battery source freshness evidence

## Provisional limit

`BATTERY_SOURCE_MAX_AGE` is provisionally 60 seconds for an environment whose
UPower refresh contract has been established. The user approved this scoped
candidate on 2026-10-28. The threshold is based on the read-only observation
below and is not evidence for other devices or states.
Unverified devices remain fail-closed. Automatic power-source preset writes
remain disabled until source transitions, consistency, and resume behavior have
verified safeguards.

## Read-only observation

- Collection lasted 10 minutes on 2026-10-28, at a two-second sampling cadence
  (301 samples).
- Each battery sample used one `Properties.GetAll` call for the selected
  physical battery. A GetAll result is one collected property set, not an
  atomic hardware measurement.
- Environment: Ubuntu 26.10, kernel `7.3.0-9-generic`, UPower `1.91.3`, one
  present physical battery (`Type=2`, `PowerSupply=true`, `IsPresent=true`).
- The observed battery remained discharging on battery throughout the valid
  window. No source transition or suspend/resume was induced.
- All 301 samples had `UpdateTime`, `Percentage`, `State`, `Energy`, and
  `EnergyFull`; no GetAll or required-property errors occurred.
- All 20 observed `UpdateTime` refresh intervals were 30 seconds. Property age
  ranged from 1.561 to 29.621 seconds (maximum 29.621 seconds). New timestamp
  values were first observed 1.561–1.621 seconds after their timestamp, within
  the two-second collection interval.
- No valid-window source changes occurred. Charging, pending states, transition
  delays, suspend/resume, near-empty behavior, and other machines remain
  uncharacterized.
- There were no missing or internally inconsistent required battery samples in
  the valid collection window. A preliminary run with an invalid battery parser
  saw a line-power state change at an indeterminate point outside the valid
  sample set; its cause is unknown, so it is not treated as transition evidence.
- Observed Percentage-to-energy-ratio difference peaked at 0.0011 percentage
  points using `(Energy - EnergyEmpty) / (EnergyFull - EnergyEmpty) * 100`.
  This is sample evidence only; no comparison tolerance is set in production.
  The proposed 1.0 percentage-point tolerance remains provisional and must be
  independently justified before enforcement.

## Runtime safeguards

The provider requires one present physical battery and a matching UPower
device, a nonzero/nonfuture `UpdateTime` no older than the provisional 60-second
limit, a consistent UPower `OnBattery` and line-power `Online` pair, finite and
ranged percentage/state values, and finite in-range `Energy` and `EnergyFull`.
Contradictory line-power devices are rejected. Reads are bounded to three
seconds. Sample age advances using
`CLOCK_BOOTTIME`, which includes suspend time; clock regression invalidates the
sample. Missing, ambiguous, stale, or inconsistent readings fail closed.

The observed 30-second refresh interval is specific to the device and
discharging state above. The implementation does not infer a refresh contract
for other devices or states; there is no runtime allowlist for verified
environments in this change. Automatic power-source writes are therefore
disabled for every environment, so these observations do not authorize
physical control. Any future write-enabling change must add a reviewed way to
identify verified environments and revalidate source freshness before each
component write.

## Safety and uncertainty

Collection used read-only UPower and sysfs operations. No hardware writes,
unplugging, suspend, or physical validation were performed. Battery serial,
vendor, model, and identifying machine paths are omitted. Evidence does not
establish charging, AC/battery transition, or resume behavior.
