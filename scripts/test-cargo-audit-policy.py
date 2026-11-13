#!/usr/bin/env python3
"""Regression tests for narrow, expiring Cargo advisory exceptions."""

import importlib.util
import json
import tempfile
import tomllib
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

    def test_incomplete_yanked_metadata_is_a_critical_audit_error(self):
        stderr = "error: couldn't check if the package is yanked: not found: No such crate in crates.io index: example"
        self.assertEqual(AUDIT.incomplete_audit_errors(stderr), [stderr])
        self.assertEqual(AUDIT.incomplete_audit_errors(""), [])
        self.assertTrue(AUDIT.audit_failed(set(), set(), {("RUSTSEC-2020-0071", "time", "0.2.22")}, {}, 0, stderr))

    def test_unsound_and_yanked_findings_fail_closed(self):
        for category in ("unsound", "yanked"):
            with self.subTest(category=category):
                self.assertTrue(AUDIT.audit_failed(set(), set(), set(), {category: [object()]}, 0, ""))
        self.assertFalse(AUDIT.audit_failed(set(), set(), set(), {"unmaintained": [object()]}, 0, ""))

    def test_real_finding_and_exception_state_are_checked(self):
        finding = ("RUSTSEC-2020-0071", "time", "0.2.22")
        self.assertTrue(AUDIT.audit_failed({finding}, set(), {finding}, {}, 1, ""))
        self.assertTrue(AUDIT.audit_failed(set(), {finding}, {finding}, {}, 1, ""))
        self.assertFalse(AUDIT.audit_failed(set(), set(), {finding}, {}, 1, ""))

    def test_audit_uses_workspace_lockfile_and_keeps_yanked_and_unsound_denials(self):
        command = AUDIT.audit_command(Path("Cargo.lock"), Path("/tmp/advisory-db"))
        self.assertIn("Cargo.lock", command)
        self.assertEqual(command.count("--deny"), 2)
        self.assertIn("unsound", command)
        self.assertIn("yanked", command)

    def test_workspace_lockfile_uses_patched_rustix_and_fixture_stays_vulnerable(self):
        root = Path(__file__).resolve().parents[1]
        locked = tomllib.loads((root / "Cargo.lock").read_text(encoding="utf-8"))["package"]
        rustix = [package["version"] for package in locked if package["name"] == "rustix"]
        self.assertIn("1.1.6", rustix)
        fixture = tomllib.loads((root / ".github/security-fixtures/RUSTSEC-2020-0071/Cargo.lock").read_text(encoding="utf-8"))["package"]
        self.assertIn(("time", "0.2.22"), {(package["name"], package["version"]) for package in fixture})


if __name__ == "__main__":
    unittest.main()
