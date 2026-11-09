# Repository reconciliation — 9 November 2026

## Baseline and preservation

Repository: `UdayaSri0/g-helper-linux`; default branch `main`; authenticated owner has ADMIN access. Workspace package version and latest published release are both `0.4.1` / `v0.4.1` despite some commit titles mentioning v0.4.2. No version change or release is authorized by this maintenance.

Fresh remote inventory: 55 branches. Initial primary heads:

| Branch | Original SHA | Relationship to original main |
|---|---|---|
| main | `7da88e9be446c623dc39b3b8a9f4de814718ddd5` | stable baseline |
| dev | `2ae7693616f11b1a32457c1b76a609c46d351445` | 0 ahead / 4 behind |
| Dev | `89c1668c3c7c17a63ae5bbb4d1cc6ca1a37f1c71` | 3 ahead / 4 behind by ancestry |

The original local checkout is on `chore/issue-62-structured-issue-forms` at `672ff31`; its modified Graphify query timestamp was preserved in place and as a binary patch. Reconciliation uses a separate worktree. All refs, remote heads, tags, PR metadata, repository metadata and the request were archived outside the tracked tree in `backups/reconcile-20261109T054304Z/`. `git bundle create ... --all` and `git bundle verify` succeeded. Detailed inventories and full validation logs are local backup artifacts rather than repository logs.

## Integration and conflict decisions

Lowercase `dev` is canonical. Starting from its original head, the integration branch fast-forwarded to original `main`, then ordinarily merged `Dev` as commit `024f6fa9c4a12a4b45c09431e051c4100ff179cd`. No conflicts occurred. Both original tips are ancestors. The resulting tree `b38abddcbe34e1afa2968aba5503f079b4d12c12` exactly equals original `main`, including source, dependencies, security policy and generated files.

`Dev`’s tree `f6250d2fb9cae865dae491eae310a2f3bd8c1568` exactly equals main ancestors `b086aac` (#131), `41f1382` (#129), and `06901e6` (#128). Its unique commit identities came from squash/merge history, not absent functionality. Current main additionally includes smart setup #132. No feature was cherry-picked or reverted; no experimental historical branch was merged wholesale. The only new file changes are maintenance/contribution documentation.

## Pull request decisions

| PR | Source → target | Decision and evidence |
|---|---|---|
| [121](https://github.com/UdayaSri0/g-helper-linux/pull/121) | docs/issue-120-release-verification → main | Superseded: checksum additions already present through release consolidation (#123/#131). README, release checklist and v0.4.0 notes match exactly; BUILD and workflow tests retain all checksum additions alongside newer work. Close after verifying this evidence; preserve source branch. |
| [133](https://github.com/UdayaSri0/g-helper-linux/pull/133) | main → Dev | Replace with validated reconciliation into lowercase dev; close only after replacement merge. Preserve Dev. |
| [131](https://github.com/UdayaSri0/g-helper-linux/pull/131) | Dev → main | Already merged; exact tree equivalence confirmed. |
| [132](https://github.com/UdayaSri0/g-helper-linux/pull/132) | chore/smart-setup → main | Already merged; current baseline includes feature. |

Both initially open PRs had no reviews, comments or unresolved threads at audit time. No reviewer approval is fabricated. New integration/promotion PR URLs and final branch SHAs are recorded in the final maintenance outcome linked from their PR descriptions; this committed report records the pre-merge evidence.

## Validation

Local commands on the reconciled runtime tree (documentation changes do not alter binaries):

```sh
cargo fmt --all -- --check
cargo build --workspace --locked
cargo test --workspace --locked
cargo clippy --workspace --all-targets --locked -- -D warnings
git diff --check
```

All passed; 393 Rust tests, zero failures. These packaging checks passed:

```sh
bash packaging/scripts/test-install-dev.sh
bash packaging/scripts/test-debian-maintainer-scripts.sh
bash packaging/scripts/test-appimage-gtk-plugin.sh
bash packaging/scripts/test-appimage-tool-integrity.sh
python3 packaging/scripts/test-release-workflow.py
python3 packaging/scripts/test_release_verification.py
python3 packaging/scripts/test_release_environment.py
python3 packaging/scripts/test-package-integration.py
python3 packaging/scripts/check-release-metadata.py
env -u DISPLAY -u WAYLAND_DISPLAY bash packaging/scripts/test-session-smoke.sh <external-evidence-directory>
```

The smoke test used isolated session/system bus addresses and fresh config, with no installed services or privileged hardware operations. UI visual testing was skipped. `packaging/scripts/build-flatpak.sh --check` was blocked locally because `flatpak-builder` is missing. Actual DEB/RPM installation, package builds and AppImage runtime testing are delegated to disposable CI runners; do not infer success from simulations.

Earlier [AppImage failure](https://github.com/UdayaSri0/g-helper-linux/actions/runs/42477596388) occurred after successful Rust compilation: four HTTP 504 download responses. Existing bounded retries and pinned SHA-256 verification remain unchanged. [Adjacent same-head CI](https://github.com/UdayaSri0/g-helper-linux/actions/runs/42475937128) passed. Final PR heads must pass fresh CI before merge.

## Security and governance

Inspected helper sender-derived PolicyKit subjects, allowed action IDs, parameter validation, fail-closed errors, package D-Bus/systemd boundaries and hardware support evidence. Runtime source and permissions are unchanged by reconciliation. This is a scoped boundary inspection, not a comprehensive vulnerability scan. No physical hardware actions were tested; production fan-curve validation allowlist remains empty.

Authenticated APIs reported `main`, `dev`, and `Dev` unprotected and no repository rulesets. Merge commits are enabled; automatic branch deletion is disabled. Default workflow token is read-only. The release-publish environment restricts the tag to v0.4.1 but does not require reviewer approval. Security-audit workflow still uses mutable action tags; most other actions are pinned. No settings were changed.

Recommended owner-approved safeguards: PR-only changes to main/dev, passing current-head core/package/security checks, resolved review threads, no force pushes/deletions, and human approvals when a reviewer is available. Protect release tags and publication separately. Existing CI already runs on pushes/PRs; existing issue forms and package/security checks were retained without adding bots or auto-merge automation.

## Complete original branch inventory

Counts below are branch ahead / behind original main. MERGED means ancestry-contained or independently verified squash incorporation; patch equivalence is assessed separately. All branches are preserved. A related PR is the latest PR for that source, not authorization to delete.

| Branch | Original SHA | Date | Ahead / behind | Classification | Related PR |
|---|---|---|---|---|---|
| `Dev` | `89c1668c3c7c17a63ae5bbb4d1cc6ca1a37f1c71` | 2026-11-07 | 3 / 4 | DUPLICATE (tree verified; ancestry preserved) | [#131](https://github.com/UdayaSri0/g-helper-linux/pull/131) |
| `UdayaSri0-patch-1` | `d695c51529c75c2c8c77a60db16942ceaabe17e6` | 2026-04-01 | 0 / 242 | MERGED | [#8](https://github.com/UdayaSri0/g-helper-linux/pull/8) |
| `chore/issue-62-structured-issue-forms` | `672ff31a850183f78246733054e8a474855c0022` | 2026-10-25 | 0 / 99 | MERGED | [#92](https://github.com/UdayaSri0/g-helper-linux/pull/92) |
| `chore/issue-orchestrator` | `b7b2a98a3e238fb1b2be82906379b6b3ed63def6` | 2026-10-25 | 0 / 96 | MERGED | [#89](https://github.com/UdayaSri0/g-helper-linux/pull/89) |
| `chore/recover-v0.4.0-release` | `57af6baf37ceeecc974d93bef37a9b2b998ff532` | 2026-10-08 | 0 / 108 | RELEASE/HISTORICAL | [#60](https://github.com/UdayaSri0/g-helper-linux/pull/60) |
| `chore/remove-v0.3.0-rpm-backfill` | `02b68fb3bfa327e3254878bc141ab6971a6aec3f` | 2026-08-15 | 1 / 151 | RELEASE/HISTORICAL | [#46](https://github.com/UdayaSri0/g-helper-linux/pull/46) |
| `chore/smart-setup` | `29084823741d871e87d4403bac4c5eab8cddef1d` | 2026-11-08 | 1 / 3 | DUPLICATE (patch-equivalent; retain) | [#132](https://github.com/UdayaSri0/g-helper-linux/pull/132) |
| `chore/v0.4.1-final-release-prep` | `1d0b39489e61854c764d531fbfd3bce442e50056` | 2026-11-06 | 2 / 10 | MERGED | [#124](https://github.com/UdayaSri0/g-helper-linux/pull/124) |
| `chore/v0.4.1-publication-gate` | `9903054e2832c207623d1d0ca84db048bcce565b` | 2026-11-06 | 1 / 9 | DUPLICATE (patch-equivalent; retain) | [#125](https://github.com/UdayaSri0/g-helper-linux/pull/125) |
| `chore/v0.4.1-release-docs` | `a54932273ae543638c11dd76e577b0a5b79f94d3` | 2026-11-05 | 5 / 12 | MERGED | — |
| `chore/v0.4.1-release-prep` | `a31409bb7bda615ee7c2f6c4ff95c6c206d62300` | 2026-11-06 | 12 / 11 | MERGED | [#123](https://github.com/UdayaSri0/g-helper-linux/pull/123) |
| `ci/baseline-clippy-appimage` | `3854d0be6bd9754dbe5cc512ab15c02a7bdcaa98` | 2026-10-25 | 4 / 100 | MERGED | [#91](https://github.com/UdayaSri0/g-helper-linux/pull/91) |
| `codex/fix-appimage-build-error` | `923b6ecdaccf7d675ddc1276f0ec46b1990140db` | 2026-04-02 | 0 / 216 | MERGED | [#21](https://github.com/UdayaSri0/g-helper-linux/pull/21) |
| `codex/fix-clippy-type_complexity-failure` | `1e9237daa52f14298fcb4f69ff82be4f36812183` | 2026-04-01 | 0 / 246 | MERGED | [#4](https://github.com/UdayaSri0/g-helper-linux/pull/4) |
| `codex/fix-clippy-type_complexity-failure-btyl2d` | `92e4ecc5e1c1e2aeab99fd90866f681f6b862aa7` | 2026-04-01 | 0 / 246 | MERGED | [#5](https://github.com/UdayaSri0/g-helper-linux/pull/5) |
| `dev` | `2ae7693616f11b1a32457c1b76a609c46d351445` | 2026-11-07 | 0 / 4 | ACTIVE (canonical, sync pending) | — |
| `docs/issue-120-release-verification` | `d174bf4e109923bd150bb477be367a026435581c` | 2026-11-05 | 1 / 12 | MERGED | [#121](https://github.com/UdayaSri0/g-helper-linux/pull/121) |
| `docs/issue-70-updater-trust-design` | `6d56e5bd08db0662c3f153d57ada475c7b8c09b0` | 2026-10-28 | 1 / 16 | DUPLICATE (patch-equivalent; retain) | [#114](https://github.com/UdayaSri0/g-helper-linux/pull/114) |
| `fix/appimage-gtk-modules` | `dd3a43c2550ad6eb0ca7dca5e1a96f2ad5973c36` | 2026-10-27 | 0 / 72 | MERGED | [#98](https://github.com/UdayaSri0/g-helper-linux/pull/98) |
| `fix/ci-session-smoke-grep` | `f7589561423bf9bc499c2b23afe3790544820ba5` | 2026-10-08 | 0 / 120 | MERGED | [#55](https://github.com/UdayaSri0/g-helper-linux/pull/55) |
| `fix/issue-119-deb-policykit` | `7b42012c505af4684665210d5b706bed26d6191d` | 2026-11-05 | 1 / 12 | MERGED | [#122](https://github.com/UdayaSri0/g-helper-linux/pull/122) |
| `fix/issue-65-evidence-matrix` | `7cc6f0c5847f55a499cdf3733a0182976c92d61d` | 2026-10-28 | 1 / 22 | DUPLICATE (patch-equivalent; retain) | [#108](https://github.com/UdayaSri0/g-helper-linux/pull/108) |
| `fix/issue-66-appimage-runtime-smoke` | `1a4444cae4193c417c9b6e520fc5a7f483319c37` | 2026-10-28 | 1 / 17 | DUPLICATE (patch-equivalent; retain) | [#113](https://github.com/UdayaSri0/g-helper-linux/pull/113) |
| `fix/issue-66-appimage-session-smoke` | `c408117d31f17fc45a0951b38cca11c19912d4d3` | 2026-10-28 | 4 / 15 | MERGED | [#115](https://github.com/UdayaSri0/g-helper-linux/pull/115) |
| `fix/issue-66-deb-lifecycle` | `ef2c295a32d8137d2175cda26894ce1b2bdba7db` | 2026-10-28 | 1 / 18 | DUPLICATE (patch-equivalent; retain) | [#112](https://github.com/UdayaSri0/g-helper-linux/pull/112) |
| `fix/issue-66-rpm-lifecycle` | `909a488321a754144704d13ad203cee12c73f248` | 2026-10-28 | 2 / 19 | DUPLICATE (patch-equivalent; retain) | [#111](https://github.com/UdayaSri0/g-helper-linux/pull/111) |
| `fix/issue-68-reversible-automation` | `1f2ad04d03294e7ef009fb12573ed9ae58fb28a3` | 2026-10-28 | 1 / 13 | DUPLICATE (patch-equivalent; retain) | [#117](https://github.com/UdayaSri0/g-helper-linux/pull/117) |
| `fix/issue-68-transition-resume` | `54f1c39a355d754918a669a178ad2d1cc684d767` | 2026-10-29 | 1 / 12 | DUPLICATE (patch-equivalent; retain) | [#118](https://github.com/UdayaSri0/g-helper-linux/pull/118) |
| `fix/issue-69-dbus-contract` | `826faf5caeea525e0650efd127206662e8914ae8` | 2026-10-28 | 1 / 23 | DUPLICATE (patch-equivalent; retain) | [#107](https://github.com/UdayaSri0/g-helper-linux/pull/107) |
| `fix/issue-70-updater-integrity` | `8fa31c6746de6a934234aac05d022eedf1d83a9f` | 2026-10-28 | 4 / 25 | MERGED | [#105](https://github.com/UdayaSri0/g-helper-linux/pull/105) |
| `fix/issue-71-tool-provenance` | `15094ad29dfdaee5bb214333ea5ece2c92c06f3a` | 2026-10-28 | 0 / 27 | MERGED | [#104](https://github.com/UdayaSri0/g-helper-linux/pull/104) |
| `fix/issue-72-nvidia-timeout` | `53c746cf941933488f92be26214fc60e9673c243` | 2026-10-28 | 0 / 64 | MERGED | [#96](https://github.com/UdayaSri0/g-helper-linux/pull/96) |
| `fix/issue-73-xrandr-timeout` | `08926b23f506e8f907ce0dfadaba65d3fb87adf0` | 2026-10-28 | 6 / 26 | DUPLICATE (patch-equivalent; retain) | [#103](https://github.com/UdayaSri0/g-helper-linux/pull/103) |
| `fix/issue-74-ignore-graphify-output` | `af046a4ad1a6b2aa753bd2bf9678883314331abf` | 2026-10-27 | 0 / 69 | MERGED | [#95](https://github.com/UdayaSri0/g-helper-linux/pull/95) |
| `fix/issue-76-install-aware-autostart` | `3f9fb2dbd70d44a4a512101bcad61f19a3f718f3` | 2026-10-28 | 0 / 33 | MERGED | [#101](https://github.com/UdayaSri0/g-helper-linux/pull/101) |
| `fix/issue-77-rustsec-ci-recovery` | `cac37ea9c0940e3d3ffabdb2e2888e47b75f93e9` | 2026-10-28 | 2 / 24 | RELEASE/HISTORICAL | [#106](https://github.com/UdayaSri0/g-helper-linux/pull/106) |
| `fix/issue-78-stale-update-action` | `5b7218ad3ca821402633acbe0d28043b20466e2e` | 2026-10-25 | 0 / 96 | MERGED | [#90](https://github.com/UdayaSri0/g-helper-linux/pull/90) |
| `fix/issue-81-role-sensor-failsafe` | `a1058d7f959f44b0095f5222bfe79193dc4e3e28` | 2026-10-28 | 1 / 20 | DUPLICATE (patch-equivalent; retain) | [#110](https://github.com/UdayaSri0/g-helper-linux/pull/110) |
| `fix/issue-82-helper-owned-curves` | `623f530ce8cf826ba838f4b13385bca390c31a32` | 2026-10-28 | 1 / 21 | DUPLICATE (patch-equivalent; retain) | [#109](https://github.com/UdayaSri0/g-helper-linux/pull/109) |
| `fix/issue-85-fan-write-support-policy` | `248faa64e9b7df74c69b592ddea0bb575b77c2cf` | 2026-10-28 | 0 / 62 | MERGED | [#97](https://github.com/UdayaSri0/g-helper-linux/pull/97) |
| `fix/issue-86-fan-curve-transaction-safety` | `65633bd5c31aeac6dc51d002e44fcf620b25536e` | 2026-10-26 | 0 / 77 | MERGED | [#94](https://github.com/UdayaSri0/g-helper-linux/pull/94) |
| `fix/issue-87-fan-write-gates` | `47b0ed2adc96dd2627537230578598828149da6d` | 2026-10-25 | 0 / 86 | MERGED | [#93](https://github.com/UdayaSri0/g-helper-linux/pull/93) |
| `fix/release-artifact-layout` | `5bb93fcaf6841f53870ce608e63554a6923e2d05` | 2026-11-06 | 1 / 6 | DUPLICATE (patch-equivalent; retain) | [#127](https://github.com/UdayaSri0/g-helper-linux/pull/127) |
| `fix/solo-release-drafts` | `5b93bfb0bdcfe048594fcaca2a1d3c7064af9389` | 2026-11-06 | 2 / 7 | MERGED | [#126](https://github.com/UdayaSri0/g-helper-linux/pull/126) |
| `fix/v0.3.0-fedora-rpm-macros` | `675b1e53a6b6f207f4e7a31804c02a9263f91821` | 2026-08-15 | 1 / 155 | DUPLICATE (patch-equivalent; retain) | [#42](https://github.com/UdayaSri0/g-helper-linux/pull/42) |
| `fix/v0.3.0-release-rpm-gate` | `826fbd0b82da12781dfae8a59db62c4ebfe0b561` | 2026-08-15 | 0 / 156 | MERGED | [#41](https://github.com/UdayaSri0/g-helper-linux/pull/41) |
| `fix/v0.3.0-rpm-backfill-checkout` | `3ef08b178334ae3f94477eb1e7011cf6c796c75d` | 2026-08-15 | 1 / 153 | RELEASE/HISTORICAL | [#44](https://github.com/UdayaSri0/g-helper-linux/pull/44) |
| `fix/v0.3.0-rpm-backfill-safe-directory` | `be95250a7f7ce017bd5122ab54af61fc2a4b99bb` | 2026-08-15 | 1 / 152 | RELEASE/HISTORICAL | [#45](https://github.com/UdayaSri0/g-helper-linux/pull/45) |
| `fix/v0.4.0-release-workflow` | `b50ab671ee0758f637a9ba8ef6b5b2f7bdf37f15` | 2026-10-08 | 1 / 110 | DUPLICATE (patch-equivalent; retain) | [#58](https://github.com/UdayaSri0/g-helper-linux/pull/58) |
| `fix/v0.4.0-tracked-dist-artifact` | `3b10a9203ca8fd1c041409b959a0f70aef50a4f7` | 2026-10-08 | 0 / 103 | MERGED | [#61](https://github.com/UdayaSri0/g-helper-linux/pull/61) |
| `main` | `7da88e9be446c623dc39b3b8a9f4de814718ddd5` | 2026-11-08 | 0 / 0 | ACTIVE (production) | [#133](https://github.com/UdayaSri0/g-helper-linux/pull/133) |
| `release/v0.3.0` | `6ff774c8a32b1b53c6f910fdcd7b5c9c73c08437` | 2026-08-15 | 0 / 160 | RELEASE/HISTORICAL | [#40](https://github.com/UdayaSri0/g-helper-linux/pull/40) |
| `release/v0.3.0-rpm-backfill` | `f53ec5b27597289b1ecb686c21cf69b8d53f3a61` | 2026-08-15 | 1 / 154 | RELEASE/HISTORICAL | [#43](https://github.com/UdayaSri0/g-helper-linux/pull/43) |
| `release/v0.3.1` | `b38f15df448d0c450bee9cfaeb3b3cb6333c5272` | 2026-08-23 | 0 / 137 | RELEASE/HISTORICAL | [#50](https://github.com/UdayaSri0/g-helper-linux/pull/50) |
| `revert-35-main` | `c0be364169ff2f4e0a606071e7cf5ac2368fa04c` | 2026-04-28 | 0 / 183 | MERGED | [#36](https://github.com/UdayaSri0/g-helper-linux/pull/36) |

Nine patch-ID outliers were reviewed semantically: final-release-prep matches #124 (5/5 changed files), release-prep #123 (21/21), AppImage session smoke #115 (4/4), updater integrity #105 (5/5), and solo release drafts #126 (10/10). Earlier release-docs is superseded by #123 acceptance reports; baseline Clippy/AppImage compressor fixes remain with newer regex/tool integrity corrections; issue-119 PolicyKit changes remain via #122/#123; issue-120 checksums remain as above. No useful missing behavior was identified.

## Retention and readiness

No branches, tags or published releases were deleted or rewritten. Ancestry-contained topic branches without open PRs are deletion candidates only after owner approval and a fresh dependency/tag check. Squash-equivalent branches require final per-branch confirmation; preserve release/historical branches and Dev. This audit does not approve deletion.

Use lowercase `dev` for new development after the reconciliation PR is merged and verified. Production promotion includes only preserved current main content and workflow documentation. Unvalidated historical work remains isolated. Final readiness remains gated on final-head CI and confirmed GitHub merges; consult the maintenance PR outcomes rather than treating this baseline report as a merge claim.
