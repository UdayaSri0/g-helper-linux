#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
advisory_db="${RUSTSEC_ADVISORY_DB:?set RUSTSEC_ADVISORY_DB to the fetched RustSec advisory database}"
cd "$repo_root"

python3 scripts/test-cargo-audit-policy.py
fixture_manifest=.github/security-fixtures/RUSTSEC-2020-0071/Cargo.toml
cargo fetch --locked --manifest-path "$fixture_manifest"
if fixture_output="$(GITHUB_STEP_SUMMARY= python3 scripts/run-cargo-audit.py --file .github/security-fixtures/RUSTSEC-2020-0071/Cargo.lock --db "$advisory_db" 2>&1)"; then
  echo "security policy accepted the known-vulnerable fixture" >&2
  exit 1
fi
if [[ "$fixture_output" != *"RustSec vulnerabilities without an exact exception:"* || "$fixture_output" != *"RUSTSEC-2020-0071"* || "$fixture_output" == *"couldn't check if the package is yanked"* ]]; then
  printf '%s\n' "$fixture_output" >&2
  echo "fixture failed without the expected advisory" >&2
  exit 1
fi
printf '%s\n' "$fixture_output"

python3 scripts/run-cargo-audit.py --db "$advisory_db"
