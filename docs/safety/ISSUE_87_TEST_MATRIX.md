# Issue 87 test matrix

| Area | Automated coverage | Hardware evidence | Status |
| --- | --- | --- | --- |
| Curve endpoint gate | Pure `rog-core` low-duty curve ending at 65°C is rejected by write policy | None permitted in automated tests | Software covered; physical blocked |
| Factory/read-only curve | Default policy accepts firmware-specific low endpoint for inspection | Read-only discovery only | Covered conservatively |
| Active re-apply | Fake hwmon with `enable_mode=1` proves zero writes | Supervised firmware semantics | Software covered; physical blocked |
| Partial point write | Existing fake-hwmon transaction/readback tests | Controlled supervised rollback | Existing software coverage; physical blocked |
| Ownership/lease recovery | Not yet implemented | Not tested | Open #80/#82 |
| Missing sensors/stalls | Not yet complete | Not tested | Open #81 |
| CPU rollback | Not yet started | Not tested | Open #83 |
| GPU readiness | Not yet started | Not tested | Open #84 |
| AppImage provenance | Not yet started | Not applicable | Open #71 |
| Package lifecycle | Not yet run in target distro matrix | Installed-runtime evidence required | Open #66 |
| Aura effects | Diagnostics/preflight only | Human observation required | Blocked #63 |

No row marked “software covered” is a claim of physical validation.
