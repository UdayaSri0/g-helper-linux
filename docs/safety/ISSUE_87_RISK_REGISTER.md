# Issue 87 risk register

| Risk | Impact | Current mitigation | Residual gate |
| --- | --- | --- | --- |
| User curve ends before verified high-temperature coverage | Thermal control may be unsafe if firmware extends duty unexpectedly | Hardware write policy rejects curves without terminal coverage before I/O | Establish device-specific evidence; do not generalize the temporary gate |
| Re-apply writes mixed points while custom mode is active | Intermediate live curve may be exposed | Re-apply is rejected before the first write | Supervised proof of an atomic/staged transition |
| Privileged rejection triggers an unintended Auto write | Active custom mode or another armed channel may be changed despite a rejected request | Recovery is armed only after snapshot and active-mode rejection; transaction recovery is scoped to the touched channel | Supervised helper-path validation |
| Generic hwmon layout mistaken for safe support | Unsupported model/firmware may receive writes | Exact ASUS device/label/layout checks plus an exact approved model/board/BIOS/kernel/driver record are required at both write boundaries; production records are empty | Supervised #64/#65 evidence and maintainer review before adding a record |
| Daemon dies while helper-owned curve remains active | Auto recovery may not run | Existing marker recovery retained | Independent authenticated lease/watchdog design and fault tests |
| Missing role sensor or fan state | Control may continue without credible thermal feedback | Not yet complete; remains an open child issue | Fake fault injection plus supervised validation |
| CI or API success mistaken for physical success | Unsafe compatibility claims | Documentation explicitly separates software and physical evidence | Dated machine record and independent sign-off |
