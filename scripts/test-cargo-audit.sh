#!/usr/bin/env bash
set -euo pipefail

repo_root="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
advisory_db="${RUSTSEC_ADVISORY_DB:?set RUSTSEC_ADVISORY_DB to the fetched RustSec advisory database}"
cd "$repo_root"

python3 scripts/test-cargo-audit-policy.py
if fixture_output="$(GITHUB_STEP_SUMMARY= python3 scripts/run-cargo-audit.py --file .github/security-fixtures/RUSTSEC-2020-0071/Cargo.lock --db "$advisory_db" 2>&1)"; then
  echo "security policy accepted the known-vulnerable fixture" >&2
  exit 1
fi
if [[ "$fixture_output" != *"RUSTSEC-2020-0071"* ]]; then
  printf '%s\n' "$fixture_output" >&2
  echo "fixture failed without the expected advisory" >&2
  exit 1
fi
printf '%s\n' "$fixture_output"

python3 scripts/run-cargo-audit.py --db "$advisory_db"
