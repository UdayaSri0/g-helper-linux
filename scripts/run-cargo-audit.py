#!/usr/bin/env python3
"""Run RustSec against a lockfile with exact, expiring advisory exceptions."""

import argparse
import datetime
import json
import os
import re
import subprocess
import sys
import tomllib
from pathlib import Path


FIELDS = {"id", "package", "version", "issue", "owner", "reason", "remediation", "expires"}


def load_exceptions(path: Path, lockfile: Path) -> dict[tuple[str, str, str], dict]:
    data = json.loads(path.read_text(encoding="utf-8"))
    if set(data) != {"exceptions"} or not isinstance(data["exceptions"], list):
        raise ValueError("policy must contain only an exceptions list")
    locked = {(p["name"], p["version"]) for p in tomllib.loads(lockfile.read_text(encoding="utf-8"))["package"]}
    exceptions = {}
    for item in data["exceptions"]:
        if not isinstance(item, dict) or set(item) != FIELDS:
            raise ValueError("each exception must contain exactly the documented fields")
        if not re.fullmatch(r"RUSTSEC-\d{4}-\d{4}", item["id"]):
            raise ValueError(f"invalid advisory id: {item['id']!r}")
        key = (item["id"], item["package"], item["version"])
        if key in exceptions:
            raise ValueError(f"duplicate exception: {key}")
        for field in ("package", "version", "issue", "owner", "reason", "remediation"):
            if not isinstance(item[field], str) or not item[field].strip():
                raise ValueError(f"{item['id']}: {field} is required")
        expiry = datetime.date.fromisoformat(item["expires"])
        if expiry <= datetime.datetime.now(datetime.timezone.utc).date():
            raise ValueError(f"{item['id']}: exception expired on {expiry}")
        if (item["package"], item["version"]) not in locked:
            raise ValueError(f"{item['id']}: {item['package']} {item['version']} is not in the lockfile")
        exceptions[key] = item
    return exceptions


def compare_findings(findings: set[tuple[str, str, str]], exceptions: dict) -> tuple[set, set]:
    return findings - exceptions.keys(), exceptions.keys() - findings


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--file", type=Path, default=Path("Cargo.lock"))
    parser.add_argument("--policy", type=Path, default=Path(".github/audit-allowlist.json"))
    parser.add_argument("--db", type=Path, required=True)
    args = parser.parse_args()

    try:
        exceptions = load_exceptions(args.policy, args.file)
        db_revision = subprocess.check_output(["git", "-C", str(args.db), "rev-parse", "HEAD"], text=True).strip()
        scanner_version = subprocess.check_output(["cargo", "audit", "--version"], text=True).strip()
        result = subprocess.run(
            ["cargo", "audit", "--json", "--deny", "unsound", "--deny", "yanked", "--file", str(args.file), "--db", str(args.db), "--no-fetch"],
            capture_output=True,
            text=True,
        )
        report = json.loads(result.stdout)
        findings = {
            (item["advisory"]["id"], item["package"]["name"], item["package"]["version"])
            for item in report["vulnerabilities"]["list"]
        }
        unexpected, stale = compare_findings(findings, exceptions)
        if unexpected:
            print("RustSec vulnerabilities without an exact exception:", file=sys.stderr)
            for advisory, package, version in sorted(unexpected):
                print(f"  {advisory}: {package} {version}", file=sys.stderr)
        if stale:
            print("stale exceptions must be removed:", file=sys.stderr)
            for advisory, package, version in sorted(stale):
                print(f"  {advisory}: {package} {version}", file=sys.stderr)
        for kind, items in report["warnings"].items():
            for item in items:
                a, p = item["advisory"], item["package"]
                print(f"RustSec {kind}: {a['id']} {p['name']} {p['version']} — {a['title']}")
        for advisory, package, version in sorted(findings & exceptions.keys()):
            print(f"RustSec exception: {advisory} {package} {version} (expires {exceptions[(advisory, package, version)]['expires']})")
        print(f"cargo-audit: {scanner_version}; advisory-db: {db_revision}")
        summary = os.environ.get("GITHUB_STEP_SUMMARY")
        if summary:
            with open(summary, "a", encoding="utf-8") as output:
                output.write(f"\n### RustSec audit\n\n- Scanner: `{scanner_version}`\n- Advisory database: `{db_revision}`\n- Vulnerabilities: {len(findings)} ({len(unexpected)} without an exact exception)\n")
                for kind, items in report["warnings"].items():
                    output.write(f"- {kind}: {len(items)}\n")
                    for item in items:
                        a, p = item["advisory"], item["package"]
                        output.write(f"  - `{a['id']}` {p['name']} {p['version']}: {a['title']}\n")
        if unexpected or stale:
            return 1
        denied_warnings = report["warnings"].get("unsound", []) + report["warnings"].get("yanked", [])
        if denied_warnings or (result.returncode and not findings):
            print(result.stderr or "cargo audit failed", file=sys.stderr)
            return 1
        return 0
    except (OSError, KeyError, TypeError, ValueError, subprocess.CalledProcessError, json.JSONDecodeError) as error:
        print(f"cargo audit policy/check failed: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
