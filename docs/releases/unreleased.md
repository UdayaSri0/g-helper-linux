# Unreleased

## Added

- Fan monitoring and safe fan controls for supported ASUS/Linux hardware.
- Dynamic fan inventory over `hwmon` with endpoint diagnostics for `fan*_input`, labels, PWM files, and RPM target files.
- Capability-driven Fans page with read-only telemetry, manual percentage control when writable, sync mode, time-limited boost, Return to Auto, and copyable diagnostics.
- Redesigned Fans page with a polished dashboard, animated RPM rotors, CPU/GPU temperature gauges, individual fan cards, disabled safe controls, curve preview, and collapsed diagnostics.
- Session DBus fan methods: `GetFanCaps`, `GetFanState`, `GetFanCurves`, `SetFanAuto`, `SetFanManualPercent`, `SetFanRpmTarget`, `SetFanCurve`, `SetFanSync`, `SetFanBoost`, and `ResetFansToAuto`.
- CLI fan diagnostics via `rog-helper fans` and `rog-helper fan-caps`.

## Safety

- Fan controls remain disabled unless the daemon reports backend support.
- The UI never writes directly to sysfs or hardware.
- Manual fan control requires acknowledgement.
- Boost is time-limited and restores Auto/BIOS mode when the timer ends.
- Dangerous fan curves are rejected by shared core validation.
