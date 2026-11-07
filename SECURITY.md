# Reporting security vulnerabilities

Do not report exploitable vulnerabilities in a public issue or pull request.
Use GitHub's **Report a vulnerability** flow for this repository to create a
private security advisory. Include the affected version or commit, a safe
reproduction, impact, and a suggested mitigation. Do not attach credentials,
personal data, or an exploit against a live system. If private reporting is
unavailable, contact the maintainer privately through a verified repository
contact channel and request a secure reporting method.

## Dependency advisory policy

CI audits the committed `Cargo.lock` using the pinned `cargo-audit` version
against the current RustSec advisory database. Each run records the database
revision. Known vulnerabilities and unsound or yanked dependencies fail CI;
unmaintained crates are reported separately for review. A clean run means only
that this scanner found no unexcepted known advisory in that lockfile against
the recorded database revision; it is not a claim that the software is free of
vulnerabilities.

An exception must be narrowly bound to one advisory, crate, and locked version,
and include a tracking issue, owner, risk reason, remediation plan, and future
expiry date in `.github/audit-allowlist.json`. Expired or stale exceptions fail
CI. Do not add blanket advisory exclusions.
