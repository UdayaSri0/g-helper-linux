#!/usr/bin/env python3
"""Regression tests for narrow, expiring Cargo advisory exceptions."""

import importlib.util
import json
import tempfile
import unittest
from pathlib import Path

SPEC = importlib.util.spec_from_file_location("run_cargo_audit", Path(__file__).with_name("run-cargo-audit.py"))
AUDIT = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(AUDIT)


class ExceptionPolicyTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.root = Path(self.temp.name)
        self.lock = self.root / "Cargo.lock"
        self.lock.write_text('version = 3\n\n[[package]]\nname = "example"\nversion = "1.2.3"\n', encoding="utf-8")
        self.policy = self.root / "policy.json"
        self.exception = {
            "id": "RUSTSEC-2026-0001", "package": "example", "version": "1.2.3",
            "issue": "#1", "owner": "maintainer", "reason": "test reason",
            "remediation": "upgrade example", "expires": "2099-01-01",
        }

    def tearDown(self):
        self.temp.cleanup()

    def save(self, entries):
        self.policy.write_text(json.dumps({"exceptions": entries}), encoding="utf-8")

    def test_accepts_exact_live_exception(self):
        self.save([self.exception])
        self.assertEqual(set(AUDIT.load_exceptions(self.policy, self.lock)), {("RUSTSEC-2026-0001", "example", "1.2.3")})

    def test_rejects_expired_or_unlocked_exception(self):
        self.exception["expires"] = "2000-01-01"
        self.save([self.exception])
        with self.assertRaises(ValueError):
            AUDIT.load_exceptions(self.policy, self.lock)
        self.exception["expires"] = "2099-01-01"
        self.exception["version"] = "9.9.9"
        self.save([self.exception])
        with self.assertRaises(ValueError):
            AUDIT.load_exceptions(self.policy, self.lock)

    def test_rejects_duplicate_exception(self):
        self.save([self.exception, self.exception])
        with self.assertRaises(ValueError):
            AUDIT.load_exceptions(self.policy, self.lock)

    def test_exception_must_match_advisory_package_and_version(self):
        exact = {("RUSTSEC-2026-0001", "example", "1.2.3"): self.exception}
        unexpected, stale = AUDIT.compare_findings({("RUSTSEC-2026-0001", "example", "9.9.9")}, exact)
        self.assertEqual(unexpected, {("RUSTSEC-2026-0001", "example", "9.9.9")})
        self.assertEqual(stale, {("RUSTSEC-2026-0001", "example", "1.2.3")})


if __name__ == "__main__":
    unittest.main()
