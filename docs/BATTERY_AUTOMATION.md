# Battery preset policy

Battery preset selection is evaluated only from a fresh, consistent physical battery source
sample. The source and the selected threshold condition each need five seconds of continuous
valid observation, measured with `CLOCK_BOOTTIME`. Invalid or stale samples reset the pending
dwell. A source change, resume, automation toggle, or manual override also requires fresh dwell.

| Power source | Charge | Policy decision |
| --- | --- | --- |
| AC | any | Select the saved AC preset after stable source dwell |
| Battery | at/below lower threshold | Select the saved Battery preset after threshold dwell |
| Battery | at/above optional return threshold with recovery preset selected | Select that recovery preset after threshold dwell |
| Battery | at/above optional return threshold without recovery preset | Report blocked/no-write; do not fall back to the AC preset |
| Battery | between thresholds | Hold the selected preset in the current session; after restart/resume, select none |
| Unknown, inconsistent, invalid, or stale | any | Reset dwell and make no selection |
| Manual override | any | Make no selection until explicit Resume and new dwell |

The return threshold and recovery preset are optional and must be configured together; the upper
threshold must be greater than the lower threshold. Existing lower-threshold-only configuration
remains valid and keeps its one-way Battery activation behavior. The 60-second freshness limit is provisional
and applies only after a device/environment refresh contract is established. Current evidence is
one discharging laptop with approximately 30-second refresh intervals; charging, source
transitions, and suspend/resume are not characterized. Unverified environments remain blocked.
The proposed 1.0 percentage-point energy/percentage tolerance is not established and is not used
to authorize automation.

Automatic hardware writes remain disabled in the daemon. The daemon has per-component three-attempt
retry scheduling (initial attempt, then five seconds and thirty seconds), generation/source
cancellation, per-operation revalidation, and readback-confirmed component status. The retry path
cannot execute backend writes while the gate is false. Policy and fake-backend tests do not enable
hardware writes and are not physical validation. Do not remove the write gate until transition,
source-consistency, and resume behavior are validated. A successful D-Bus method response alone is
not confirmation.
