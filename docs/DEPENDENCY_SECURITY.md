# Cargo dependency security review

## Audit baseline

The baseline is the pre-remediation `Cargo.lock` from `main` at `2e1643e487de476f66b81bae5dea7aeef8b0f3cf`, scanned with `cargo-audit 0.22.2` and RustSec advisory database commit `99670cfc0bc574433034b75876a4b791ccaa2efb` (updated 2026-10-27). It contained 14 vulnerability findings and 7 warnings. This reproduces the missing dependency-audit coverage recorded by issue #77 at `6414c3e`; findings are advisory matches, not a claim that every vulnerable code path is enabled or reachable.

## Vulnerability remediation matrix

| Advisory | Locked package before → after | Dependency path and impact | Fixed version | Decision, compatibility, and residual exposure |
|---|---|---|---|---|
| RUSTSEC-2026-0338 | quinn-proto 0.11.14 → 0.11.18 | `reqwest` → `quinn` → `quinn-proto`; remote peer can panic unordered stream reads with empty STREAM frames. Ordered reads are unaffected. | 0.11.18 | Updated lockfile. HTTP/3 is not enabled in the current Linux feature set, so this code path is not active in the app; dependency remains for Reqwest's optional transport graph. Workspace tests cover compatibility, not QUIC exploitability. |
| RUSTSEC-2026-0342 | quinn-proto 0.11.14 → 0.11.18 | A zero-length QUIC DATAGRAM can bypass receive-buffer accounting; network denial of service if the affected transport is active. | 0.11.17 | Updated to 0.11.18. HTTP/3 remains disabled; no runtime QUIC test was performed. |
| RUSTSEC-2026-0339 | quinn-proto 0.11.14 → 0.11.18 | A peer can grow pending connection-ID retirements without bound; network memory exhaustion if affected QUIC behavior is used. | 0.11.17 | Updated to 0.11.18. HTTP/3 remains disabled; no runtime QUIC test was performed. |
| RUSTSEC-2026-0185 | quinn-proto 0.11.14 → 0.11.18 | CVSS 7.5: remote peers can cause memory exhaustion through unbounded out-of-order stream reassembly. | 0.11.15 | Updated to 0.11.18. HTTP/3 remains disabled; no runtime QUIC test was performed. |
| RUSTSEC-2026-0340 | quinn-proto 0.11.14 → 0.11.18 | CVSS 7.5: a peer can trigger a panic when ACK_FREQUENCY transmission is enabled. | 0.11.18 | Updated to 0.11.18. HTTP/3 remains disabled; no runtime QUIC test was performed. |
| RUSTSEC-2026-0348 | rustls 0.23.37 → 0.23.46 | Reqwest TLS; with rustls `brotli` enabled, an unauthenticated peer can cause up to 1 GiB certificate-decompression allocation. Current Reqwest feature set does not enable `brotli`. | 0.23.46 | Updated to 0.23.46. TLS remains enabled; the affected Brotli feature is currently absent. |
| RUSTSEC-2026-0346 | rustls 0.23.37 → 0.23.46 | Reqwest TLS; local privileged access and high-complexity conditions are needed to expose client-session secrets through `Debug`. | 0.23.46 | Updated to 0.23.46. No app API change was needed; full workspace validation checks the dependency update. |
| RUSTSEC-2026-0349 | rustls 0.23.37 → 0.23.46 | Reqwest TLS; a network attacker under high-complexity conditions can exploit an unauthenticated ServerHello to downgrade later connections to classical key exchange. | 0.23.46 | Updated to 0.23.46. No protocol settings were relaxed. |
| RUSTSEC-2026-0285 | rustls 0.23.37 → 0.23.46 | Reqwest TLS; crafted TLS 1.3 handshake messages crossing encryption levels could be accepted, affecting confidentiality. | 0.23.45 | Updated to 0.23.46. TLS continues to use rustls; no custom cryptography was introduced. |
| RUSTSEC-2026-0104 | rustls-webpki 0.103.10 → 0.103.16 | Reqwest certificate validation; malformed certificate revocation-list data can trigger a reachable parser panic. | 0.103.13 | Updated to 0.103.16. No custom certificate parsing was added. |
| RUSTSEC-2026-0099 | rustls-webpki 0.103.10 → 0.103.16 | Reqwest certificate validation; a certificate with wildcard names could incorrectly pass name-constraints checks. | 0.103.12 | Updated to 0.103.16. Normal TLS name validation remains enabled. |
| RUSTSEC-2026-0098 | rustls-webpki 0.103.10 → 0.103.16 | Reqwest certificate validation; URI name constraints could be incorrectly accepted. | 0.103.12 | Updated to 0.103.16. Normal TLS name validation remains enabled. |
| RUSTSEC-2026-0382 | rustls-webpki 0.103.10 → 0.103.16 | Reqwest certificate validation may accept a distrusted intermediate certificate. | 0.103.16 | Updated to 0.103.16, the first fixed release in the current 0.103 line. |
| RUSTSEC-2026-0381 | webpki-roots 1.0.6 → 1.0.10 | Reqwest's bundled CA roots could trust distrusted emSign intermediates, affecting certificate-chain trust. | 1.0.10 | Updated to 1.0.10. System and bundled trust configuration were otherwise unchanged. |

All changes are lockfile-only, use advisory-patched releases, and passed the workspace build, tests, formatting, and Clippy checks. The quinn-proto update requires rand 0.10.3; project code has no direct rand usage. No intentionally vulnerable code was deleted or disabled to make the scan pass.

## Warning remediation and remaining items

| Advisory | Package before → after | Type and path | Action and residual risk |
|---|---|---|---|
| RUSTSEC-2026-0190 | anyhow 1.0.101 → 1.0.103 | Unsound `Error::downcast_mut`; direct workspace dependency. | Patched by lockfile update. No project use of `downcast_mut` was found. |
| RUSTSEC-2026-0221 | event-listener 5.4.1 → 5.4.2 | Unsound cross-thread `!Send` stack tags; transitive through zbus async-broadcast. | Patched by lockfile update; workspace tests/build pass. |
| RUSTSEC-2026-0097 | rand 0.9.2 → 0.10.3 | Unsound custom-logger interaction; present in the optional Quinn graph. | Updated to the version required by quinn-proto 0.11.18; no direct project rand API use. |
| RUSTSEC-2026-0386 | tokio 1.49.0 → 1.53.3 | Unsound `AsyncFd` constructors; Tokio is used throughout the workspace. | Patched by lockfile update. No direct `AsyncFd` use was found; transitive users still receive the patched crate. |
| RUSTSEC-2026-0371 | zbus_names 4.3.1 | Unmaintained standalone crate; zbus 5 dependency graph. | Remains as a warning. Upstream folded this crate into zbus 6; migration is a major-version change and is not required to remove a known vulnerability. Track a separately tested zbus 6 migration. |
| RUSTSEC-2026-0370 | zvariant 5.9.2 | Unmaintained standalone crate; zbus 5 dependency graph. | Same zbus 6 migration constraint; no known vulnerability is reported by this advisory. |
| RUSTSEC-2026-0372 | zvariant_derive 5.9.2 | Unmaintained derive crate; zbus 5 dependency graph. | Same zbus 6 migration constraint; no known vulnerability is reported by this advisory. |

The remaining three unmaintained-crate warnings are surfaced in CI but do not fail the vulnerability gate. CI fails on unexcepted vulnerabilities, unsound dependencies, and yanked releases. There are currently no advisory exceptions. A clean scan is limited to the exact lockfile and advisory database revision recorded in that run.
