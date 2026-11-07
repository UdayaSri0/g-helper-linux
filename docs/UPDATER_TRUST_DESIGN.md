# Updater Trust and Release Signing Design (Proposal)

**Status:** design proposal only. This document does not enable automatic
replacement, provision keys, or claim that a signed update service exists.

## Current behavior

The About page can check GitHub Releases and open the latest release page for a
manual download. Automatic replacement is disabled until release metadata can
be authenticated with a pinned public key. No signing key or signing service is
provisioned. Keep this behavior until the prerequisites below are met.

## Goals

- Authenticate release metadata independently of GitHub account and publishing
  credentials.
- Bind each artifact to the application, release version, channel, OS,
  architecture, artifact type, exact filename, byte length, and digest.
- Detect expired or rolled-back metadata, and support key rotation and recovery
  after a signing-key compromise.
- Bound network and disk use, validate the downloaded executable, and leave the
  installed file intact whenever verification or replacement fails.
- Keep manual release-page downloads usable if automatic verification is
  unavailable.

## Non-goals

- This proposal does not create or embed a production private key.
- GitHub-hosted SHA-256 files or checksums from the same release account are not
  independent authentication.
- The updater will not modify package-managed installations, request elevated
  privileges, or change fan, firmware, or other hardware settings.
- A valid signature does not prove that the software is bug-free or safe to
  install; release review and CI remain necessary.

## Approaches considered

1. **Manual-only updates.** This is the current safe behavior and has the least
   implementation and key-custody risk. It does not provide authenticated
   in-app artifact verification.
2. **Detached signed manifest.** A pinned public key can verify a compact
   manifest containing artifact digests and platform metadata. This is smaller
   than a repository update framework, but the project would own metadata
   freshness, rollback prevention, root-key rotation, and compromise recovery.
3. **The Update Framework (TUF) metadata.** TUF provides separate root,
   targets, snapshot, and timestamp roles, expiry, versioning, signature
   thresholds, and a defined root-rotation flow. It adds client and release
   process complexity, but best matches the required key rotation and
   compromise-recovery properties. **Recommendation:** evaluate a maintained
   Rust TUF client and adopt TUF if its compatibility and maintenance status
   pass a separate implementation review. Until then, remain manual-only.

TUF role separation and metadata properties are described in the
[TUF roles and metadata guide](https://theupdateframework.com/docs/metadata/),
the [TUF security design](https://theupdateframework.io/docs/security/), and
the [TUF specification](https://theupdateframework.github.io/specification/).
Do not implement a project-specific signature format or cryptographic
primitive in place of a reviewed implementation.

## Proposed trust and signing model

- If TUF is selected, the application ships with reviewed, pinned TUF root
  metadata. No key fetched from GitHub may silently replace this bootstrap
  trust.
- Root keys should be held offline, independently from source control, build
  runners, release-publishing tokens, and application distribution accounts.
  Use a documented custodian quorum for root changes (proposed: two of three
  maintainers); maintainers must approve the actual quorum and custody method.
- Targets signing keys authorize specific release artifacts. They must be
  separate from ordinary CI and release-publishing credentials. Prefer
  hardware-backed signing with two-person authorization; build and publish
  jobs receive no private signing key.
- Any online timestamp-signing identity must be narrowly scoped, separately
  managed, and unable to authorize target content or change trusted keys.
  Prefer short-lived credentials and hardware-backed service custody. If this
  separation cannot be established, do not enable automatic updates.
- Publish signed metadata and artifacts as immutable release outputs. A
  publisher may upload bytes but cannot create trusted metadata. Record the
  source commit, artifact digest, metadata versions, signer identity, and
  approval evidence in the release record.

## Metadata and artifact policy

Each target entry must bind the exact release version and channel to a
supported platform tuple (Linux OS, architecture, ABI where relevant), artifact
type, filename, signed size, and SHA-256 digest. The verifier must reject
unknown channels, duplicate or ambiguous matches, unsupported architectures,
unexpected names, and version downgrade. The downloaded ELF must match the
expected class and machine; any additional ABI checks must be based on the
actual supported runtime contract.

TUF metadata must be signature-checked, version-monotonic, unexpired, and
consistent across root, targets, snapshot, and timestamp. Persist trusted
metadata versions atomically. Reject rollback, freeze, mix-and-match, malformed
metadata, and an invalid signature. Persisted client state is user-writable;
the design must not claim rollback protection against an attacker who can
modify the same user's application data or executable.

## Download and replacement sequence

1. Use HTTPS to a configured release origin. Reject credentials, non-HTTPS
   schemes, origin changes, and redirects unless an explicitly reviewed
   same-origin rule permits them. Bound connect time, total time, metadata
   bytes, artifact bytes, and temporary disk usage.
2. Verify TUF metadata and select exactly one target matching the installed
   application's identity, release channel, OS, architecture, artifact type,
   and version policy before downloading the binary.
3. Stream the artifact into a uniquely created temporary file in the same
   directory/filesystem as the eligible user-local executable. Enforce the
   signed byte length and a separately configured hard maximum while
   streaming; reject truncation, extra bytes, timeout, or write error.
4. Verify the target digest and expected ELF format/architecture before any
   replacement. Flush and close the temporary file, then use an atomic
   replacement strategy with a tested rollback path. Preserve the old
   executable on all failures and clean up temporary files.
5. Restrict replacement to a regular, user-owned, user-writable executable
   selected by the existing installation policy. Never follow an attacker-
   controlled symlink, use `sudo`, or replace a package-managed installation.
6. Report which stage failed in user-visible language and retain the manual
   GitHub release-page action. Failure to retrieve or verify metadata must
   never silently fall back to unsigned replacement.

Timeout values, hard size limits, supported architectures, and any ABI checks
must be selected from measured release artifacts and reviewed before
implementation; this proposal does not invent production limits.

## Rotation and compromise recovery

- Root rotation follows the TUF versioned-root process: each new root is
  authorized by the previous root threshold and the new root threshold. Ship a
  tested recovery path before the first production root expires.
- Targets-key rotation is authorized by the root role. Revoke the affected key,
  publish new metadata, and stop accepting metadata signed only by the
  compromised key.
- Timestamp-key compromise requires rapid revocation and replacement. Keep the
  timestamp role unable to sign targets or modify root trust.
- If the root threshold is compromised, clients cannot safely trust an
  ordinary update from that repository. Recovery requires a separately trusted
  application distribution/update channel or a user-installed release that
  carries a new reviewed root. Until maintainers test and document that route,
  automatic replacement remains disabled.
- Maintain a rehearsed incident process: revoke/rotate, publish corrected
  metadata, invalidate affected metadata state as required by the TUF client,
  communicate through independent channels, and preserve forensic records.

## Required verification before enabling automatic replacement

- Review and pin the selected TUF client version and its dependency graph;
  confirm its metadata parsing, expiry, rollback, rotation, and signature
  behavior from its tests and documentation.
- Test valid and invalid signatures, expired metadata, rollback and
  mix-and-match metadata, root and target key rotation, compromised target and
  timestamp keys, unsupported architecture, wrong artifact type, wrong digest,
  truncated and oversized downloads, timeout, redirect, disk-full, failed
  rename, and rollback after replacement failure using local fixtures.
- Confirm the previous executable is byte-for-byte intact on every rejected
  download or failed replacement path. Exercise repeated updates and recovery
  after process interruption.
- Demonstrate release signing with a test-only key that is not available to
  normal CI or publishing jobs. Do not use test keys in a production build.
- Independently review key generation, custody, quorum, access logging,
  revocation, rotation, backup, recovery, and signer separation from build and
  publishing credentials.
- Verify manual update checks and release-page fallback still work when the
  metadata host is unavailable, expired, malformed, or untrusted.

## Decision gate and remaining blockers

This proposal recommends TUF; it does not authorize production deployment.
Before implementation, maintainers must approve the framework, custodian
quorum, trust-root bootstrap and recovery channel, supported target matrix,
metadata hosting, expiration policy, and measured download caps. The project
currently has no provisioned signer or production root. Until the design is
approved, signing custody exists, and all verification tests pass, automatic
executable replacement must stay disabled and issue #70 must remain open.
