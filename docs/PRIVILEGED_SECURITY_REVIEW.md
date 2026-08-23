# Privileged Architecture Security Review

This review covers the `Dev` implementation after the CPU, fan, keyboard/Aura lighting, and
battery privilege migration. It is a source and packaging audit, not a claim of hardware
certification. No UI or session-daemon process runs as root.

## 1. Trust boundary

```text
unprivileged user session                         root system service

GTK UI
  | typed session-DBus calls
  v
rog-helperd ── preferred asusd/supergfxd/direct providers
  |
  | typed, path-free system-DBus calls
  v
system bus ── peer credentials ──> rog-helper-privileged
                                      |
                                      | fixed action + caller unique bus name
                                      v
                                   PolicyKit
                                      |
                                      | authorization for this category/check
                                      v
                         re-discover + validate fixed kernel ABI
                                      |
                                      v
                   approved CPU / ASUS fan / ASUS LED / battery sysfs
```

The UI and daemon are untrusted with respect to root. The system bus supplies the helper's direct
caller's unique name; callers do not submit an identity. PolicyKit resolves that `system-bus-name`
subject from bus credentials. The daemon is the direct system-bus caller because it is the
unprivileged routing boundary. The helper never trusts a session-bus name, PID, UID, executable
name, path, or authorization token supplied as a method argument.

## 2. Privileged method inventory

| Method | Input | Fixed authorization | Root effect |
|---|---|---|---|
| `Ping` | none | none; diagnostic | No write |
| `GetVersion` | none | none; diagnostic | No write |
| `GetCapabilities` | none | none; diagnostic | No write |
| `CanPerform` | one of four allow-listed action IDs | non-interactive check of caller | No write and no prompt |
| `SetCpuTurbo` | boolean | `cpu.control` | Fixed detected turbo attribute |
| `SetCpuPowerMode` | `Quiet`, `Balanced`, or `Performance` | `cpu.control` | Detected governor/EPP attributes |
| `SetCpuGovernor` | detected exact token | `cpu.control` | Detected policy governors |
| `SetCpuEpp` | detected exact token | `cpu.control` | Detected policy EPP values |
| `SetCpuFrequencyLimits` | optional MHz minimum/maximum | `cpu.control` | Detected policy frequency attributes |
| `SetCpuCoreOnline` | logical CPU ID and boolean | `cpu.control` | Detected `cpuN/online` |
| `SetFanAuto` | empty/all or fixed `asus-wmi:{cpu,gpu,mid}` ID | `fans.control` | Verified ASUS WMI reset ABI |
| `SetFanCurve` | fixed fan ID and exactly eight `(u8,u8)` points | `fans.control` | Verified ASUS WMI curve ABI |
| `ResetFansToAuto` | none | `fans.control` | Verified ASUS WMI reset ABI |
| `SetKeyboardBacklightBrightness` | integer level | `lighting.control` | Canonical ASUS WMI keyboard LED |
| `SetAuraEffect` | allow-listed mode, two validated RGB strings, allow-listed speed/direction/zone | `lighting.control` | Fixed ASUS Aura HID reports through `/dev/rog-helper-aura` |
| `SetBatteryChargeLimit` | percentage | `battery.control` | One exact standard battery threshold |

There is no generic filesystem-write, process-execution, GPU, PCI, kernel-module, ACPI, USB, HID,
environment, or arbitrary-byte privileged method.

## 3. PolicyKit actions

| Action | Methods | Defaults |
|---|---|---|
| `io.github.roghelper.cpu.control` | all CPU writes | any: no; inactive/active: `auth_admin` |
| `io.github.roghelper.fans.control` | ASUS fan curve/Auto writes | any: no; inactive/active: `auth_admin` |
| `io.github.roghelper.lighting.control` | keyboard brightness fallback | any: no; inactive/active: `auth_admin` |
| `io.github.roghelper.battery.control` | standard threshold fallback | any: no; inactive/active: `auth_admin` |

Actions are separate by category and are fixed in each write method. `auth_admin_keep` is not used,
so the shipped policy does not deliberately retain an authorization for later writes. The unused
broad `io.github.roghelper.system.configure` action was removed during this review.

## 4. Root-accessed resources

- CPU: `/sys/devices/system/cpu/intel_pstate/no_turbo`,
  `/sys/devices/system/cpu/cpufreq/boost`, exact numeric `policyN` governor/EPP/min/max attributes,
  and exact numeric `cpuN/online` attributes.
- Fans: fixed `pwm1..3_enable` and eight fixed temperature/PWM point pairs only on a hwmon device
  whose `name`, hardware labels, point layout, and canonical `device` identity resolve to
  `asus-nb-wmi`.
- Lighting: `brightness` and `max_brightness` under the canonical
  `asus::kbd_backlight` ASUS WMI LED. Native Aura writes use only the root-owned
  `/dev/rog-helper-aura` alias, after revalidating DMI, USB VID/PID, interface number, kernel
  driver, HID descriptor hash/report shape, canonical device identity, and the opened file
  descriptor. The caller cannot provide a path or raw report bytes.
- Battery: `type` and `charge_control_end_threshold` under one unambiguous power-supply device
  reporting exact `type=Battery`.
- Fail-safe state: root-owned `/run/rog-helper/fan-control-active` in a mode `0700` systemd runtime
  directory.
- IPC: the system D-Bus Unix socket and PolicyKit authority.

No external command is executed by `rog-helper-privileged`.

## 5. Input and filesystem validation

- CPU governor and EPP values must match every affected policy's detected allow-list exactly.
- Frequency values must fit `u32`, be inside intersected hardware bounds, and cannot invert either
  the requested pair or an active policy when only one side changes.
- Logical CPU IDs must be currently detected; CPU 0 cannot be offlined.
- CPU endpoint leaves and directory shapes are allow-listed, final attributes must be regular
  non-symlink files, and canonical paths must remain under the CPU sysfs hierarchy.
- Fan IDs are a fixed three-value semantic set. Curves require eight ordered, monotonic points,
  30–100 °C and 0–100%, including the high-temperature safe floor. Raw PWM is derived internally.
- Every fan write rechecks the ASUS WMI identity, channel number, endpoint name, regular-file type,
  canonical parent, and readback. Failure attempts an Auto reset.
- Keyboard brightness must be `0..=max_brightness`; identity, fixed maximum, canonical parent, and
  regular attribute type are rechecked immediately before the write and readback is required.
- Battery limits use the shared `20..=100` contract. Multiple devices (including one hot-plugged
  after discovery), non-batteries, unexpected types, final symlinks, canonical escapes,
  disappearing endpoints, and failed readback are rejected.
- CPU, fan, lighting, and battery writes require readback before success is returned.
- Parameters are validated before PolicyKit where possible and are validated again at write time.

Sysfs class directories legitimately contain kernel-owned ancestor symlinks. Those are allowed only
after canonical identity checks. Final writable attributes are not allowed to be symlinks. Local
unprivileged users cannot create or replace entries in the approved `/sys` or `/run/rog-helper`
hierarchies.

## 6. systemd hardening

Enabled directives include `NoNewPrivileges`, `PrivateTmp`, `PrivateDevices`, `ProtectHome`,
`ProtectSystem=strict`, `ProtectKernelModules`, `ProtectControlGroups`, `ProtectKernelLogs`,
`ProtectClock`, `ProtectHostname`, `ProtectProc=invisible`, `ProcSubset=pid`,
`RestrictAddressFamilies=AF_UNIX`, `RestrictNamespaces`, `RestrictSUIDSGID`, `RestrictRealtime`,
`LockPersonality`, `MemoryDenyWriteExecute`, native syscall architecture,
`SystemCallFilter=@system-service`, `UMask=0077`, and a mode `0700` runtime directory. Only the
approved sysfs trees are declared in `ReadWritePaths`. `PrivateDevices=yes` remains enabled; only
the optional root-only Aura alias is introduced into the private device namespace with
`BindPaths=-/dev/rog-helper-aura` and admitted by the matching narrow `DeviceAllow` entry.

`Restart=on-failure` restores the service after a crash or kill. On restart, normal shutdown, or
idle exit, an armed fan marker causes an Auto reset attempt. A temporarily missing fan backend does
not trap the service in a restart loop: the marker is retained and idle exit is deferred for retry.

Deliberate exceptions:

- `ProtectKernelTunables=yes` is not compatible with the helper's required sysfs writes, so it
  remains `no`. This means the systemd mount namespace alone cannot make every other sysfs file
  read-only; typed methods, semantic discovery, fixed roots, and endpoint validation are the
  primary boundary. `ReadWritePaths` records and preserves the required trees under the other
  filesystem protections but is not claimed as an exclusive sysfs allow-list.
- A read-only root filesystem cannot be applied to the approved sysfs leaves; all other filesystem
  content remains covered by `ProtectSystem=strict`.
- `CapabilityBoundingSet`/`AmbientCapabilities` are not set. Cross-driver sysfs store handlers do not
  provide a stable capability contract, and some kernels perform driver-specific checks. UID 0 is
  retained inside the sandbox rather than claiming an unverified capability-only design.
- `ProtectProc=invisible` and `ProcSubset=pid` are compatible because helper-side CPU code uses
  control discovery only; CPU telemetry remains in the unprivileged daemon.

## 7. Attack surfaces reviewed

- GTK/session-DBus request construction and malformed maps
- daemon direct-first/fallback decisions and external-daemon preference
- system-bus ownership and send policy
- DBus sender extraction and PolicyKit subject construction
- action separation, interaction flags, cancellation, denial, and unavailable agents
- all root method signatures and error mappings
- numeric conversion, string allow-lists, fixed IDs, curve policy, and readback
- symlink, file-type, canonical-parent, disappearance, and TOCTOU behavior
- fan partial-write recovery, process death, idle exit, and restart recovery
- root process environment and command execution
- systemd sandbox and writable-path exceptions
- exact root-only Aura udev alias, private-device bind, and device-cgroup allow-list
- Debian, RPM, Arch, tarball, system-D-Bus, systemd, and PolicyKit install metadata and modes
- operation without the helper and preference for asusd/supergfxd/direct writes

## 8. Findings fixed

1. Replaced `auth_admin_keep` with per-check `auth_admin` for all hardware actions.
2. Removed the unused broad `system.configure` action from policy and the probe allow-list.
3. Updated packaging validation to require the battery action, reject retained authorization, and
   assert the privileged service hardening baseline.
4. Added exact CPU endpoint-shape, regular-file, canonical-root, and partial-limit inversion checks.
5. Added final-open ASUS fan endpoint validation and symlink rejection.
6. Added immediate keyboard and battery attribute identity/type revalidation.
7. Moved CPU/fan semantic rejection before interactive authorization while retaining write-time
   validation.
8. Added service crash restart and a private runtime directory for fan fail-safe recovery.
9. Prevented the root service from taking logging configuration from its process environment.
10. Added CPU readback, battery hot-plug ambiguity revalidation, and symlink-safe fan-marker writes.
11. Prevented a targeted fan reset from clearing the global recovery marker and kept the helper
    online to retry Auto recovery when a fan backend temporarily disappears.
12. Made package payload modes independent of the builder umask; rendered systemd/D-Bus metadata
    is explicitly `0644` and the root-owned helper remains `0755`.
13. Bound only `/dev/rog-helper-aura` into the helper's private device namespace and retained a
    root-only udev alias with no `MODE`, `GROUP`, or `uaccess` grant.
14. Routed session D-Bus activation through the packaged `Type=dbus` user service while retaining
    the standard unprivileged `Exec` fallback.
15. Removed PATH lookup from portable privileged activation by fixing the documented `/usr/local`
    tarball helper path to `/usr/local/libexec/rog-helper-privileged`.

## 9. Remaining risks

- The daemon is the helper's direct system-bus subject. Another process running as the same desktop
  user can ask that user's session daemon to start an operation and therefore can cause a clearly
  labelled PolicyKit prompt. It cannot bypass PolicyKit or change the fixed action/method input.
- Authorization and the subsequent kernel write cannot be one atomic operation. Endpoints are
  rediscovered/revalidated immediately before write, but hardware can still disappear between open
  and completion; errors are returned and state is refreshed.
- A non-graceful fan-helper death cannot execute cleanup at the instant of death. The root-only
  marker plus `Restart=on-failure` restores Auto on service restart; the validated curve safe floor
  limits the interim state. Power loss remains firmware/BIOS territory.
- Multi-policy CPU writes can be partially applied if hardware disappears between individual sysfs
  writes. The UI/daemon reports failure and refreshes actual state; no optimistic success is shown.
- Full UID 0 remains necessary for compatibility until supported kernels and drivers have a tested,
  narrower capability contract.
- Because `ProtectKernelTunables` must remain disabled, a memory-safety or logic vulnerability in
  the root process has more sysfs reach than its typed API. Rust memory safety, the absence of
  `unsafe` in the helper, the syscall/sandbox controls, and strict endpoint validation reduce but
  do not erase that consequence.
- Real PolicyKit-agent behavior and every supported hardware ABI still require manual distro/device
  testing before release.

## 10. Compatibility

The helper remains optional. Telemetry, asusd profiles/battery controls, supergfxd GPU switching,
safe Aura APIs, and directly writable kernel controls continue without it. Only a supported direct
write that fails for permission can route to the matching typed helper method. Missing helper,
PolicyKit, or authorization does not stop the UI or telemetry.

## 11. Verification

Completed in this review:

- `cargo fmt --all -- --check`
- `cargo build --workspace` and a release-mode workspace build used for package inspection
- `cargo test --workspace` (146 tests)
- `cargo clippy --workspace --all-targets -- -D warnings`
- release-metadata, shell-syntax, XML, desktop-entry, and AppStream validation
- Debian build plus archive ownership/mode inspection; root integration is `root:root`, the helper
  is `0755`, and its root-consumed metadata is `0644`
- portable tarball build, numeric root ownership/mode inspection, and absolute privileged
  executable verification
- offline `systemd-analyze security` (`3.5 OK` on systemd 255)
- two isolated session-daemon start/stop cycles with no privileged helper; diagnostics remained
  responsive and correctly reported the helper as unavailable
- graph refresh with `graphify update .`

Unit tests cover malformed values, allow-lists, bounds, fixed IDs, symlink escapes, missing and
hot-plugged endpoints, direct preference, helper failure, authorization errors, and fallback
behavior. Real root/helper kill during a fan transaction, PolicyKit-agent absence, interactive
success/cancellation/denial, UI kill, and physical-hardware disappearance still require supervised
manual testing on a packaged target before release. They were not simulated with unsafe host writes
in this source review.

## 12. Change accounting

Use `git diff --stat` from the reviewed `Dev` worktree. Generated `graphify-out` changes should be
reported separately from product and packaging changes.
