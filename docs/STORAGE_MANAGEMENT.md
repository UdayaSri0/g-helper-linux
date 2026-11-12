# Storage Management

Storage Management is read-only. The GTK page receives storage snapshots from
`rog-helperd` over the session D-Bus API; it does not open block devices or run
SMART commands. The CLI uses the same `rog-providers` implementation for local
diagnostics. No storage action is exposed by the privileged helper.

Stage 2 adds read-only topology per block namespace. Sysfs supplies parent-child
relationships, sector geometry, mount attribution, and system-storage evidence.
UDisks2 read-only ObjectManager metadata confirms partition tables and
partition identity when available. A topology remains `uncertain` when those
sources disagree or are incomplete, and is `unavailable` when UDisks2 cannot
be reached. The existing
drive metrics and volume monitoring continue in that state. `Protected`,
`Uncertain`, and `PotentiallyManageable` are informational classifications;
none authorizes a storage operation. The current provider reports Protected
where it has positive evidence and otherwise remains Uncertain; it does not
yet assert PotentiallyManageable. An unmounted partition is never assumed safe
to modify.

The partition map is grouped by namespace within each persistent drive
dashboard. It uses byte offsets and lengths only after geometry validation.
Only validated interior gaps are shown as unallocated; filesystem free bytes
are never drawn as unallocated disk space. Selection and details remain
read-only. The map does not claim that a partition can be mounted, formatted,
resized, or deleted.

The GTK page stacks a full-width dashboard for every detected physical drive
below a compact global summary. Each dashboard is retained by opaque device ID
and owns its metrics, history graphs, health and volume rows, and technical
details expander. Telemetry updates change these widgets in place. Only
confirmed inventory changes add or remove dashboards, in stable device order.
Transient unavailable or stale snapshots leave the last valid inventory
visible, while readings whose own timestamps expire are shown as unavailable.
There is one vertical scroller, and live refresh does not reset its position or
collapse technical details. The global application navigation stays visible.

The compact overview summarizes physical-drive count, summed raw physical
capacity, evidence-based aggregate health, and combined current read/write
throughput. Unknown or unsupported health is never counted as healthy. The
per-drive dashboards show identity, temperature, capacity, health, read and
write throughput and IOPS, independent bounded histories, supported SMART
attributes, and each drive's partitions and volumes. Technical information is
expandable; essential health information stays visible. Partition size remains
separate from measured filesystem capacity. Usage bars appear only when both
used and available bytes are present. Long model names wrap or ellipsize within
their dashboard and remain available in a tooltip or technical details. Opening
or refreshing the page performs no privileged request.

## Data flow and identity

`rog-providers::storage::StorageProvider` enumerates whole physical block
devices under `/sys/block`, excludes virtual block devices, and groups NVMe
namespaces by controller. It hashes a hardware identifier where available to
form the opaque ID used across samples. If the kernel exposes no stable
identifier, the sysfs topology is the fallback and identity may change after
reconnection. Partition rows belong to their containing drive, never to the
physical drive count. Device mapper, encrypted, RAID, and other logical stacks
are not attributed to a physical drive unless the relation is unambiguous.

Capacity is the sum of a drive's physical namespaces. Filesystem use is
reported separately for mounted volumes, using mountinfo to identify mounts
and `statvfs` for filesystem capacity, used, and available bytes. Unmounted,
locked, or unattributed volumes have no invented usage percentage. Multiple mounts of the same
filesystem are shown as one volume with multiple mount paths.

## Refresh and freshness

Block I/O counters come from `/sys/block/<name>/stat`. Kernel sector counters
are multiplied by 512 bytes regardless of logical block size. Throughput and
IOPS are deltas divided by monotonic elapsed time. The first sample, reset
counters, missing reads, and long sampling gaps have no rate. Histories are
bounded to 60 samples per drive. The daemon samples I/O about once a second;
volume usage refreshes every 15 seconds. A removed drive leaves the current
inventory and its history is discarded.

NVMe temperature is read only from hwmon sensors linked to that controller.
Each reading carries its source and timestamp. A missing or stale temperature
is unavailable, never zero. Other sensor families are omitted until reliable
physical attribution is available.

Health runs separately from fast telemetry, about every ten minutes, and is
cached. UDisks2's existing SMART cache is preferred where it reports a recent
supported result. Optional `smartctl` JSON is a bounded, timed, read-only
fallback. Background health checks avoid waking rotational drives. A healthy
label requires a positive supported result; missing or blocked SMART stays
unknown, unsupported, permission denied, or stale as appropriate. NVMe
percentage used is endurance consumption, not a universal health score.

`GetStorageState` returns compact `a{sv}` state with health summary, while
`GetStorageDiagnostics` includes health attributes. Neither accepts a device
path or exposes a write operation. Opaque IDs and redacted diagnostics avoid
disclosing serials; local mount paths are shown only where useful to the user.

## Availability and packaging

Basic inventory and I/O work without UDisks2 or smartmontools. SMART detail is
optional and may be limited by host permissions, hardware support, USB bridge
behavior, or Flatpak sandbox access. The Flatpak manifest does not gain broad
device or filesystem access for this feature. No package requires a SMART tool
at installation. None of the storage views mounts, repairs, formats, trims,
benchmarks, or runs a SMART self-test.

## Verification

Stage 2 synthetic tests exercise sector geometry, GPT/MBR tables, interior
unallocated extents, overlap and overflow rejection, system mount and swap
classification, namespace attribution, and absent or inconsistent UDisks2
metadata. Run `cargo test --workspace --locked` with the locked format, build,
and Clippy checks. The session smoke uses isolated D-Bus buses and does not
perform storage writes. Manual acceptance should compare the map and
classification against `lsblk`, sysfs, UDisks2, and mountinfo on disposable or
known devices; it must not treat a fixture pass as physical validation.

Provider fixtures cover discovery, namespace grouping, mount attribution,
counter arithmetic and missing data. SMART JSON tests cover supported and
malformed responses, warnings and missing attributes. Workspace formatting,
build, tests, Clippy, package checks, and manual hardware comparisons must be
recorded before integration. Real hardware acceptance should compare drive
count, model, capacity, I/O counters, temperature source, volume usage, and
SMART values against trusted host sources without generating a benchmark or
write load. A fixture pass alone is not physical validation.

### Local validation, 2026-11-10

- Locked workspace format, build, 419 tests, and Clippy with warnings denied passed.
- Isolated session D-Bus calls to both storage methods succeeded. The compact
  method omitted SMART attributes; diagnostics included them without a serial.
- A live read-only CLI check found two NVMe drives: `nvme0n1` (1,024,209,543,168
  bytes) and `nvme1n1` (1,000,204,886,016 bytes). Models and capacities matched
  sysfs. Attributed NVMe hwmon readings and mounted-volume `statvfs` values were
  compared with host sources; ordinary activity produced nonzero write I/O.
- UDisks2 cached SMART reported a healthy result for both drives. This verifies
  the cached backend on these drives, not all SMART fields or SATA/USB hardware.
- Debian package build, payload/security check, and isolated lifecycle tests
  passed; portable tarball built. Packaging script tests and isolated UI launch
  smoke passed, but the latter did not assert visual or interactive behavior.
- Flatpak build check could not run because `flatpak-builder` is absent. RPM
  build tools and AppImage prerequisites are absent locally. CI and manual GTK
  visual/hardware acceptance remain required before promotion to `main`.
